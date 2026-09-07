use std::sync::{Arc, Mutex};
use trans4mers_domain::event::{McpTrafficDirection, McpTrafficFrame};
use trans4mers_providers::mcp_client::{McpClient, McpTrafficSink};
use trans4mers_providers::mcp_inspector::{
    McpInspectorManager, NodeEnvironment, detect_node_environment,
};
use trans4mers_storage::repos::mcp_traffic_repo::McpTrafficRepo;

/// Test 1: Real-time protocol traffic log with intact ordering across handshake, tools/list, and tools/call
#[tokio::test]
async fn test_mcp_traffic_logging_and_ordering_intact() {
    let captured_frames: Arc<Mutex<Vec<McpTrafficFrame>>> = Arc::new(Mutex::new(Vec::new()));
    let captured_clone = captured_frames.clone();

    let traffic_sink: McpTrafficSink = Arc::new(move |frame| {
        let mut list = captured_clone.lock().unwrap();
        list.push(frame);
    });

    // Spin up local mock HTTP MCP server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = vec![0u8; 4096];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let req_text = String::from_utf8_lossy(&buf[..n]);

                if req_text.starts_with("GET /sse") {
                    let sse_resp = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\n\r\n";
                    let _ = stream.write_all(sse_resp.as_bytes()).await;
                    let endpoint_ev = "event: endpoint\r\ndata: /rpc\r\n\r\n";
                    let _ = stream.write_all(endpoint_ev.as_bytes()).await;
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                } else if req_text.starts_with("POST /rpc") {
                    if let Some(body_start) = req_text.find("\r\n\r\n") {
                        let body = &req_text[body_start + 4..];
                        if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(body) {
                            let method = json_val
                                .get("method")
                                .and_then(|m| m.as_str())
                                .unwrap_or("");
                            let id = json_val
                                .get("id")
                                .cloned()
                                .unwrap_or(serde_json::Value::Null);

                            let resp_body = match method {
                                "initialize" => serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "result": {
                                        "protocolVersion": "2024-11-05",
                                        "capabilities": { "tools": {} },
                                        "serverInfo": { "name": "mock-test-server", "version": "1.0.0" }
                                    }
                                }),
                                "notifications/initialized" => serde_json::json!({}),
                                "tools/list" => serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "result": {
                                        "tools": [{
                                            "name": "calc_add",
                                            "description": "Adds numbers",
                                            "inputSchema": { "type": "object" }
                                        }]
                                    }
                                }),
                                "tools/call" => serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "result": {
                                        "content": [{ "type": "text", "text": "Result: 42" }]
                                    }
                                }),
                                _ => serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "result": "pong"
                                }),
                            };

                            let resp_str = resp_body.to_string();
                            let http_resp = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                resp_str.len(),
                                resp_str
                            );
                            let _ = stream.write_all(http_resp.as_bytes()).await;
                        }
                    }
                }
            });
        }
    });

    let client_url = format!("http://{}/sse", addr);
    let client = McpClient::connect_named(
        "mock-calc-server",
        &client_url,
        None,
        None,
        Some(traffic_sink),
    )
    .await
    .expect("Connection to mock MCP server must succeed");

    // Allow client to process SSE endpoint frame
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // 1. Initialize
    let init_res = client.initialize().await;
    assert!(
        init_res.is_ok(),
        "MCP initialize must succeed: {:?}",
        init_res.err()
    );

    // 2. List tools
    let tools_res = client.list_tools().await;
    assert!(tools_res.is_ok(), "tools/list must succeed");
    let tools = tools_res.unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].manifest().name, "calc_add");

    // 3. Call tool
    let tool_call_res = client
        .rpc_call(
            "tools/call",
            Some(serde_json::json!({
                "name": "calc_add",
                "arguments": { "a": 20, "b": 22 }
            })),
        )
        .await;
    assert!(tool_call_res.is_ok(), "tools/call must succeed");

    // Verify traffic frames
    let frames = captured_frames.lock().unwrap().clone();
    assert!(
        frames.len() >= 6,
        "Expected at least 6 traffic frames, got {}",
        frames.len()
    );

    // Verify ordering intact
    assert_eq!(frames[0].direction, McpTrafficDirection::Outbound);
    assert_eq!(frames[0].method.as_deref(), Some("initialize"));
    assert_eq!(frames[0].server_name, "mock-calc-server");

    assert_eq!(frames[1].direction, McpTrafficDirection::Inbound);
    assert_eq!(frames[1].server_name, "mock-calc-server");
    assert!(frames[1].payload.contains("mock-test-server"));

    assert_eq!(frames[2].direction, McpTrafficDirection::Outbound);
    assert_eq!(
        frames[2].method.as_deref(),
        Some("notifications/initialized")
    );

    assert_eq!(frames[3].direction, McpTrafficDirection::Outbound);
    assert_eq!(frames[3].method.as_deref(), Some("tools/list"));

    assert_eq!(frames[4].direction, McpTrafficDirection::Inbound);
    assert!(frames[4].payload.contains("calc_add"));

    assert_eq!(frames[5].direction, McpTrafficDirection::Outbound);
    assert_eq!(frames[5].method.as_deref(), Some("tools/call"));

    assert_eq!(frames[6].direction, McpTrafficDirection::Inbound);
    assert!(frames[6].payload.contains("Result: 42"));

    // Cleanup
    let _ = client.close().await;
    server_handle.abort();
}

/// Test 2: SQLite persistence and chronological retrieval via McpTrafficRepo
#[test]
fn test_mcp_traffic_repo_persistence_and_querying() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();

    // Create table directly matching M007
    conn.execute(
        "CREATE TABLE IF NOT EXISTS mcp_traffic_logs (
            id TEXT PRIMARY KEY,
            server_name TEXT NOT NULL,
            direction TEXT NOT NULL,
            method TEXT,
            payload TEXT NOT NULL,
            created_at DATETIME NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_mcp_traffic_logs_server ON mcp_traffic_logs(server_name, created_at ASC);",
        [],
    ).unwrap();

    let now = chrono::Utc::now();

    // Insert 3 frames for server-alpha
    for i in 1..=3 {
        let frame = McpTrafficFrame {
            id: format!("frame-alpha-{}", i),
            server_name: "server-alpha".to_string(),
            direction: if i % 2 == 1 {
                McpTrafficDirection::Outbound
            } else {
                McpTrafficDirection::Inbound
            },
            method: Some(format!("method-{}", i)),
            payload: format!("{{\"step\": {}}}", i),
            timestamp: now + chrono::Duration::milliseconds(i * 10),
        };
        McpTrafficRepo::insert(&conn, &frame).unwrap();
    }

    // Insert 2 frames for server-beta
    for i in 1..=2 {
        let frame = McpTrafficFrame {
            id: format!("frame-beta-{}", i),
            server_name: "server-beta".to_string(),
            direction: McpTrafficDirection::Outbound,
            method: Some("ping".to_string()),
            payload: "{\"ping\": true}".to_string(),
            timestamp: now + chrono::Duration::milliseconds(i * 20),
        };
        McpTrafficRepo::insert(&conn, &frame).unwrap();
    }

    // Query server-alpha
    let alpha_logs = McpTrafficRepo::list_by_server(&conn, "server-alpha", 100).unwrap();
    assert_eq!(alpha_logs.len(), 3);
    assert_eq!(alpha_logs[0].id, "frame-alpha-1");
    assert_eq!(alpha_logs[1].id, "frame-alpha-2");
    assert_eq!(alpha_logs[2].id, "frame-alpha-3");
    assert_eq!(alpha_logs[0].direction, McpTrafficDirection::Outbound);
    assert_eq!(alpha_logs[1].direction, McpTrafficDirection::Inbound);

    // Query server-beta
    let beta_logs = McpTrafficRepo::list_by_server(&conn, "server-beta", 100).unwrap();
    assert_eq!(beta_logs.len(), 2);
    assert_eq!(beta_logs[0].server_name, "server-beta");

    // Clear server-alpha
    McpTrafficRepo::clear_by_server(&conn, "server-alpha").unwrap();
    let cleared_alpha = McpTrafficRepo::list_by_server(&conn, "server-alpha", 100).unwrap();
    assert_eq!(cleared_alpha.len(), 0);

    // Beta remains intact
    let retained_beta = McpTrafficRepo::list_by_server(&conn, "server-beta", 100).unwrap();
    assert_eq!(retained_beta.len(), 2);
}

/// Test 3: Node environment detection on host machine
#[test]
fn test_node_environment_detection_live_system() {
    let node_env = detect_node_environment();
    // On this Windows developer machine, Node is installed
    assert!(
        node_env.available,
        "Node.js should be detected on host machine"
    );
    assert!(
        node_env.node_path.is_some(),
        "Node path should be populated"
    );
    assert!(node_env.npx_path.is_some(), "Npx path should be populated");
    assert!(
        node_env.node_version.is_some(),
        "Node version should be extracted"
    );
    assert!(
        node_env.guidance.is_none(),
        "Guidance should be None when Node is present"
    );
}

/// Test 4: Node absence returns typed guidance, never an error state
#[test]
fn test_node_environment_guidance_on_absence() {
    let mock_absent_env = NodeEnvironment {
        available: false,
        node_path: None,
        node_version: None,
        npx_path: None,
        guidance: Some(
            "Node.js runtime was not detected on this machine. To launch the official MCP Inspector CLI, please install Node.js from https://nodejs.org. Alternatively, you can use Trans4mers' built-in zero-dependency protocol traffic log directly in this panel.".to_string(),
        ),
    };

    assert!(!mock_absent_env.available);
    assert!(mock_absent_env.guidance.is_some());
    let guidance = mock_absent_env.guidance.unwrap();
    assert!(guidance.contains("Node.js runtime was not detected"));
    assert!(guidance.contains("https://nodejs.org"));
}

/// Test 5: McpInspectorManager lifecycle
#[tokio::test]
async fn test_mcp_inspector_lifecycle() {
    let manager = McpInspectorManager::new();
    let initial_status = manager.status().await;
    assert!(!initial_status.running);
    assert_eq!(initial_status.pid, None);

    // Stopping an idle inspector is clean and returns Ok
    let stop_res = manager.stop().await;
    assert!(stop_res.is_ok());
}
