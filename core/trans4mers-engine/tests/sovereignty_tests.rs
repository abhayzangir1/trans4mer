use serde_json::json;
use std::sync::Arc;
use trans4mers_domain::ids::ProjectId;
use trans4mers_engine::diff_reviewer::DiffReviewer;
use trans4mers_engine::mcp_server_bridge::McpServerBridge;
use trans4mers_engine::symbolic_compressor::SymbolicCompressor;
use trans4mers_storage::db_handle::DbHandle;

#[test]
fn test_diff_reviewer_hunks_and_secrets() {
    let before = "database_url = \"postgres://localhost:5432/db\"";
    let after = "database_url = \"postgres://localhost:5432/db\"\napi_key = \"sk_live_secretkey1234567890\"";

    let (hunks, adds, dels) = DiffReviewer::compute_file_hunks(before, after);
    assert_eq!(adds, 1);
    assert_eq!(dels, 0);
    assert!(!hunks.is_empty(), "Diff hunks should be computed");

    let patterns = vec!["api_key".to_string(), "sk_live_".to_string()];
    let secret_found = DiffReviewer::scan_secrets(after, &patterns);
    let msg = secret_found.expect("Secret scanner must detect sensitive token");
    assert!(msg.contains("api_key") || msg.contains("sk_live_"));
}

#[test]
fn test_symbolic_compressor_heuristic() {
    let raw_text = "database_url = postgres://localhost:5432/db\nerror[E0308]: mismatched types\n";
    let facts = SymbolicCompressor::compress_heuristic(raw_text);

    assert!(
        !facts.is_empty(),
        "Symbolic extraction should generate facts"
    );
    assert!(
        facts
            .iter()
            .any(|f| f.subject == "database_url" && f.predicate == "equals")
    );
    assert!(facts.iter().any(|f| f.predicate == "encountered_error"));

    let formatted = SymbolicCompressor::format_markdown(&facts);
    assert!(
        formatted.contains("database_url"),
        "Formatted context should retain subject"
    );
}

#[tokio::test]
async fn test_mcp_bridge_protocol() {
    let unique = format!("t4m_test_{}.sqlite", uuid::Uuid::new_v4());
    let db_path = std::env::temp_dir().join(unique);
    let db = DbHandle::open(&db_path).expect("open db");

    let event_bus = Arc::new(trans4mers_engine::event_bus::EventBus::new(10));
    let (sched, _, _) = trans4mers_engine::scheduler::Scheduler::new(4);
    let registry = Arc::new(trans4mers_domain::provider::ProviderRegistry::new());
    let shutdown = Arc::new(trans4mers_engine::shutdown_manager::ShutdownManager::new());
    let config = trans4mers_domain::config::AppConfig::default();

    let state = Arc::new(trans4mers_engine::app_state::AppState::new(
        db,
        event_bus,
        Arc::new(sched),
        registry,
        shutdown,
        config,
        Arc::new(trans4mers_engine::skill_loader::SkillCatalog::default()),
        Arc::new(trans4mers_domain::browser::NoopBrowserManager),
    ));

    let project_id = ProjectId::new();

    // 1. Test initialize
    let init_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {}
    })
    .to_string();

    let init_resp = McpServerBridge::handle_request(state.clone(), project_id.clone(), &init_req)
        .await
        .unwrap();
    let init_val: serde_json::Value = serde_json::from_str(&init_resp).unwrap();
    assert_eq!(init_val["jsonrpc"], "2.0");
    assert_eq!(
        init_val["result"]["serverInfo"]["name"],
        "trans4mers-sovereign-bridge"
    );

    // 2. Test tools/list
    let list_req = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    })
    .to_string();

    let list_resp = McpServerBridge::handle_request(state.clone(), project_id.clone(), &list_req)
        .await
        .unwrap();
    let list_val: serde_json::Value = serde_json::from_str(&list_resp).unwrap();
    let tools = list_val["result"]["tools"].as_array().expect("tools array");
    assert!(tools.iter().any(|t| t["name"] == "memory_search"));
    assert!(tools.iter().any(|t| t["name"] == "deep_research"));
    assert!(tools.iter().any(|t| t["name"] == "browser_navigate"));

    // 3. Test unknown method
    let bad_req = json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "non_existent_method",
        "params": {}
    })
    .to_string();

    let bad_resp = McpServerBridge::handle_request(state, project_id, &bad_req)
        .await
        .unwrap();
    let bad_val: serde_json::Value = serde_json::from_str(&bad_resp).unwrap();
    assert_eq!(bad_val["error"]["code"], -32601);

    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_filesystem_list_tool() {
    use trans4mers_domain::tool::Tool;

    let tmp_dir = std::env::temp_dir().join(format!("t4m_fs_list_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&tmp_dir).unwrap();
    std::fs::write(tmp_dir.join("file_a.txt"), "hello").unwrap();
    std::fs::write(tmp_dir.join("file_b.txt"), "world").unwrap();
    std::fs::create_dir(tmp_dir.join("sub_dir")).unwrap();

    let tool = trans4mers_engine::native_tools::FilesystemListTool::new();
    let req = trans4mers_domain::tool::ToolRequest {
        tool_name: "filesystem.list".to_string(),
        execution_id: trans4mers_domain::ids::ExecutionId::new(),
        requesting_agent_id: trans4mers_domain::ids::AgentInstanceId::new(),
        project_id: ProjectId::new(),
        workspace_root: tmp_dir.clone(),
        arguments: json!({ "path": "." }),
        idempotency_key: None,
    };

    let res = tool.execute(&req).await.expect("execute filesystem.list");
    assert!(res.success);
    assert!(res.content.contains("file_a.txt"));
    assert!(res.content.contains("file_b.txt"));
    assert!(res.content.contains("sub_dir/"));

    let _ = std::fs::remove_dir_all(&tmp_dir);
}
