use chrono::Utc;
use serde_json::json;
use std::sync::Arc;
use trans4mers_domain::config::AppConfig;
use trans4mers_domain::conversation::{Conversation, ConversationSettings, ConversationStatus};
use trans4mers_domain::ids::{AgentInstanceId, ConversationId, ExecutionId, ProjectId};
use trans4mers_domain::memory::MemoryTier;
use trans4mers_domain::provider::ProviderRegistry;
use trans4mers_domain::tool::{Tool, ToolRequest};
use trans4mers_engine::app_state::AppState;
use trans4mers_engine::event_bus::EventBus;
use trans4mers_engine::native_tools::{
    MemoryArchiveTool, MemoryInsertTool, MemoryReplaceTool, MemorySearchTool,
};
use trans4mers_engine::scheduler::Scheduler;
use trans4mers_engine::shutdown_manager::ShutdownManager;
use trans4mers_storage::db_handle::DbHandle;
use trans4mers_storage::migration_runner::MigrationRunner;
use trans4mers_storage::repos::{conversation_repo, memory_repo};
use uuid::Uuid;

fn setup_test_context() -> (
    Arc<AppState>,
    ProjectId,
    AgentInstanceId,
    std::path::PathBuf,
) {
    let unique = format!("t4m_memory_upgrade_{}.sqlite", Uuid::new_v4());
    let db_path = std::env::temp_dir().join(unique);
    let db = DbHandle::open(&db_path).expect("open db");

    db.with_exclusive_conn(|conn| MigrationRunner::run_project_migrations(conn, 768))
        .expect("run project migrations");

    let project_id = ProjectId::new();
    let agent_id = AgentInstanceId::new();

    // Insert agent instance to satisfy FKs
    db.with_write_tx(|conn| {
        conn.execute(
            "INSERT INTO agent_instances (id, project_id, definition_id, status, capabilities, depth_level, created_at, updated_at)
             VALUES (?1, ?2, 'test-def', 'Active', '[]', 0, datetime('now'), datetime('now'))",
            rusqlite::params![agent_id.as_str(), project_id.as_str()],
        )?;
        Ok(())
    }).expect("insert test agent instance");

    let conv = Conversation {
        id: ConversationId::new(),
        project_id,
        title: "Test Conversation".to_string(),
        status: ConversationStatus::Active,
        settings: ConversationSettings::default(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    db.with_write_tx(|conn| conversation_repo::insert_conversation(conn, &conv))
        .expect("insert test conversation");

    let event_bus = Arc::new(EventBus::new(20));
    let (sched, _, _) = Scheduler::new(4);
    let registry = Arc::new(ProviderRegistry::new());
    let shutdown = Arc::new(ShutdownManager::new());
    let config = AppConfig::default();

    let state = Arc::new(AppState::new(
        db.clone(),
        event_bus.clone(),
        Arc::new(sched),
        registry,
        shutdown,
        config,
        Arc::new(trans4mers_engine::skill_loader::SkillCatalog::default()),
        Arc::new(trans4mers_domain::browser::NoopBrowserManager),
    ));

    state.project_dbs.insert(project_id, Arc::new(db));
    state.project_event_buses.insert(project_id, event_bus);

    (state, project_id, agent_id, db_path)
}

#[tokio::test]
async fn test_memory_replace_invalidates_and_links() {
    let (state, project_id, agent_id, _path) = setup_test_context();

    let insert_tool = MemoryInsertTool::new(state.clone());
    let replace_tool = MemoryReplaceTool::new(state.clone());
    let search_tool = MemorySearchTool::new(state.clone());

    // 1. Insert original memory
    let insert_req = ToolRequest {
        tool_name: "memory.insert".to_string(),
        arguments: json!({
            "content": "User prefers dark mode with high contrast.",
            "tier": "Working",
            "importance": 0.8
        }),
        requesting_agent_id: agent_id,
        project_id,
        execution_id: ExecutionId::new(),
        workspace_root: std::env::temp_dir(),
        idempotency_key: None,
    };

    let res = insert_tool.execute(&insert_req).await.expect("insert tool");
    assert!(res.success);
    // Parse memory ID from "Stored new memory '<id>' in tier 'Working'."
    let old_id = res.content.split('\'').nth(1).expect("extract id");

    // Verify it exists in DB
    let db = state.get_project_db(&project_id).unwrap();
    let mem1 = db
        .with_read_conn(|conn| memory_repo::get_project_memory(conn, old_id))
        .unwrap()
        .expect("mem1 exists");
    assert!(mem1.valid_at.is_some());
    assert!(mem1.invalid_at.is_none());
    assert!(mem1.replaced_by.is_none());

    // 2. Replace with contradicting/revised fact
    let replace_req = ToolRequest {
        tool_name: "memory.replace".to_string(),
        arguments: json!({
            "old_memory_id": old_id,
            "new_content": "User switched preference to light mode.",
            "tier": "Semantic",
            "importance": 0.95
        }),
        requesting_agent_id: agent_id,
        project_id,
        execution_id: ExecutionId::new(),
        workspace_root: std::env::temp_dir(),
        idempotency_key: None,
    };

    let rep_res = replace_tool
        .execute(&replace_req)
        .await
        .expect("replace tool");
    assert!(rep_res.success);
    // Extract new ID from output
    let new_id = rep_res.content.split('\'').nth(3).expect("extract new id");

    // 3. Verify prior memory is non-destructively invalidated and linked
    let mem1_after = db
        .with_read_conn(|conn| memory_repo::get_project_memory(conn, old_id))
        .unwrap()
        .expect("mem1 still exists");
    assert!(
        mem1_after.invalid_at.is_some(),
        "Old memory must have invalid_at timestamp set"
    );
    assert_eq!(
        mem1_after.replaced_by.as_deref(),
        Some(new_id),
        "Old memory replaced_by must point to new ID"
    );

    // Verify new memory is active
    let mem2 = db
        .with_read_conn(|conn| memory_repo::get_project_memory(conn, new_id))
        .unwrap()
        .expect("mem2 exists");
    assert!(mem2.valid_at.is_some());
    assert!(mem2.invalid_at.is_none());
    assert_eq!(mem2.tier, MemoryTier::Semantic);
    assert_eq!(mem2.content, "User switched preference to light mode.");

    // 4. Default retrieval must return ONLY active memories (excludes mem1)
    let search_req = ToolRequest {
        tool_name: "memory.search".to_string(),
        arguments: json!({
            "query": "mode preference",
            "temporal": false
        }),
        requesting_agent_id: agent_id,
        project_id,
        execution_id: ExecutionId::new(),
        workspace_root: std::env::temp_dir(),
        idempotency_key: None,
    };

    let search_res = search_tool.execute(&search_req).await.expect("search tool");
    assert!(
        search_res
            .content
            .contains("User switched preference to light mode.")
    );
    assert!(
        !search_res
            .content
            .contains("User prefers dark mode with high contrast.")
    );
}

#[tokio::test]
async fn test_temporal_retrieval_returns_windows() {
    let (state, project_id, agent_id, _path) = setup_test_context();

    let insert_tool = MemoryInsertTool::new(state.clone());
    let replace_tool = MemoryReplaceTool::new(state.clone());
    let search_tool = MemorySearchTool::new(state.clone());

    // Insert fact
    let ins_res = insert_tool
        .execute(&ToolRequest {
            tool_name: "memory.insert".to_string(),
            arguments: json!({ "content": "Database hosted on Postgres 15." }),
            requesting_agent_id: agent_id,
            project_id,
            execution_id: ExecutionId::new(),
            workspace_root: std::env::temp_dir(),
            idempotency_key: None,
        })
        .await
        .unwrap();
    let old_id = ins_res.content.split('\'').nth(1).unwrap();

    // Replace fact
    replace_tool
        .execute(&ToolRequest {
            tool_name: "memory.replace".to_string(),
            arguments: json!({
                "old_memory_id": old_id,
                "new_content": "Database migrated to SQLite 3 sovereign store."
            }),
            requesting_agent_id: agent_id,
            project_id,
            execution_id: ExecutionId::new(),
            workspace_root: std::env::temp_dir(),
            idempotency_key: None,
        })
        .await
        .unwrap();

    // Query with temporal: true
    let temporal_req = ToolRequest {
        tool_name: "memory.search".to_string(),
        arguments: json!({
            "query": "Database",
            "temporal": true
        }),
        requesting_agent_id: agent_id,
        project_id,
        execution_id: ExecutionId::new(),
        workspace_root: std::env::temp_dir(),
        idempotency_key: None,
    };

    let search_res = search_tool.execute(&temporal_req).await.unwrap();
    assert!(
        search_res.content.contains("INVALIDATED"),
        "Temporal search should list INVALIDATED status"
    );
    assert!(
        search_res.content.contains("ACTIVE"),
        "Temporal search should list ACTIVE status"
    );
    assert!(
        search_res
            .content
            .contains("Database hosted on Postgres 15.")
    );
    assert!(
        search_res
            .content
            .contains("Database migrated to SQLite 3 sovereign store.")
    );
}

#[tokio::test]
async fn test_write_time_deduplication_increments_retrieval_count() {
    let (state, project_id, agent_id, _path) = setup_test_context();

    let insert_tool = MemoryInsertTool::new(state.clone());

    let fact_text = "Rust toolchain version is 1.85.";

    // Insert fact first time
    let res1 = insert_tool
        .execute(&ToolRequest {
            tool_name: "memory.insert".to_string(),
            arguments: json!({ "content": fact_text }),
            requesting_agent_id: agent_id,
            project_id,
            execution_id: ExecutionId::new(),
            workspace_root: std::env::temp_dir(),
            idempotency_key: None,
        })
        .await
        .unwrap();
    assert!(res1.content.contains("Stored new memory"));
    let mem_id = res1.content.split('\'').nth(1).unwrap();

    // Insert fact second time (exact duplicate text)
    let res2 = insert_tool
        .execute(&ToolRequest {
            tool_name: "memory.insert".to_string(),
            arguments: json!({ "content": fact_text }),
            requesting_agent_id: agent_id,
            project_id,
            execution_id: ExecutionId::new(),
            workspace_root: std::env::temp_dir(),
            idempotency_key: None,
        })
        .await
        .unwrap();

    assert!(
        res2.content
            .contains("matched content hash. Reinforced retrieval count to 1"),
        "Duplicate write should reinforce rather than duplicate"
    );

    // Assert only ONE record exists in the database
    let db = state.get_project_db(&project_id).unwrap();
    let all_mems = db
        .with_read_conn(|conn| memory_repo::list_all_by_project(conn, &project_id.to_string()))
        .unwrap();
    assert_eq!(
        all_mems.len(),
        1,
        "Exactly one row should exist after deduplication"
    );
    assert_eq!(all_mems[0].id.as_str(), mem_id);
    assert_eq!(all_mems[0].retrieval_count, 1);
}

#[tokio::test]
async fn test_memory_archive_tool() {
    let (state, project_id, agent_id, _path) = setup_test_context();

    let insert_tool = MemoryInsertTool::new(state.clone());
    let archive_tool = MemoryArchiveTool::new(state.clone());
    let search_tool = MemorySearchTool::new(state.clone());

    let ins_res = insert_tool
        .execute(&ToolRequest {
            tool_name: "memory.insert".to_string(),
            arguments: json!({ "content": "Temporary token expires tomorrow." }),
            requesting_agent_id: agent_id,
            project_id,
            execution_id: ExecutionId::new(),
            workspace_root: std::env::temp_dir(),
            idempotency_key: None,
        })
        .await
        .unwrap();
    let mem_id = ins_res.content.split('\'').nth(1).unwrap();

    // Archive it
    let arch_res = archive_tool
        .execute(&ToolRequest {
            tool_name: "memory.archive".to_string(),
            arguments: json!({ "memory_id": mem_id, "reason": "No longer needed" }),
            requesting_agent_id: agent_id,
            project_id,
            execution_id: ExecutionId::new(),
            workspace_root: std::env::temp_dir(),
            idempotency_key: None,
        })
        .await
        .unwrap();
    assert!(arch_res.success);

    // Verify in DB that invalid_at is set, but replaced_by is None
    let db = state.get_project_db(&project_id).unwrap();
    let mem = db
        .with_read_conn(|conn| memory_repo::get_project_memory(conn, mem_id))
        .unwrap()
        .expect("archived memory exists in DB for audit");
    assert!(mem.invalid_at.is_some());
    assert!(mem.replaced_by.is_none());

    // Default search excludes archived memory
    let search_res = search_tool
        .execute(&ToolRequest {
            tool_name: "memory.search".to_string(),
            arguments: json!({ "query": "Temporary token", "temporal": false }),
            requesting_agent_id: agent_id,
            project_id,
            execution_id: ExecutionId::new(),
            workspace_root: std::env::temp_dir(),
            idempotency_key: None,
        })
        .await
        .unwrap();
    assert!(search_res.content.contains("No matching memories found."));
}
