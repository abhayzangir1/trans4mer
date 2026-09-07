use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Notify;

use trans4mers_providers::mcp_client::McpClient;

#[tokio::test]
async fn test_mcp_invalid_url_scheme_rejected() {
    // 1. Rejects non-http/https scheme
    let res = McpClient::connect_with_auth("ftp://example.com", None, None).await;
    assert!(res.is_err(), "FTP scheme should be rejected");
    let err_msg = res.err().unwrap().to_string();
    assert!(
        err_msg.contains("Unsupported remote MCP URL scheme"),
        "Unexpected error: {}",
        err_msg
    );

    // 2. Rejects empty URL
    let res_empty = McpClient::connect_with_auth("   ", None, None).await;
    assert!(res_empty.is_err(), "Empty URL should be rejected");
}

#[tokio::test]
async fn test_mcp_streamable_http_handshake_and_tools_list() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let ready_notify = Arc::new(Notify::new());
    let ready_clone = ready_notify.clone();

    // Spawn mock MCP HTTP/SSE server
    let server_task = tokio::spawn(async move {
        // Step 1: Accept GET for SSE
        let (mut sse_stream, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 2048];
        let n = sse_stream.read(&mut buf).await.unwrap();
        let req_text = String::from_utf8_lossy(&buf[..n]);
        let req_lower = req_text.to_lowercase();
        assert!(req_lower.starts_with("get "));
        assert!(req_lower.contains("accept: text/event-stream"));
        assert!(req_lower.contains("authorization: bearer secret-mcp-token"));

        // Send 200 OK SSE headers + endpoint event
        let sse_header = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\n\r\n";
        sse_stream.write_all(sse_header.as_bytes()).await.unwrap();

        // Emit endpoint event directing client POST to /messages
        let endpoint_event = "event: endpoint\r\ndata: /messages\r\n\r\n";
        sse_stream
            .write_all(endpoint_event.as_bytes())
            .await
            .unwrap();

        ready_clone.notify_one();

        // Step 2: Accept POST for JSON-RPC
        let (mut post_stream, _) = listener.accept().await.unwrap();
        let mut post_buf = [0u8; 4096];
        let n = post_stream.read(&mut post_buf).await.unwrap();
        let post_text = String::from_utf8_lossy(&post_buf[..n]);
        let post_lower = post_text.to_lowercase();

        assert!(post_lower.starts_with("post /messages"));
        assert!(post_lower.contains("authorization: bearer secret-mcp-token"));
        assert!(post_text.contains("\"method\":\"tools/list\""));

        // Extract request id
        let json_start = post_text.find('{').unwrap();
        let req_json: serde_json::Value = serde_json::from_str(&post_text[json_start..]).unwrap();
        let req_id = req_json.get("id").unwrap().as_u64().unwrap();

        // Send 200 OK with tools list
        let resp_payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": req_id,
            "result": {
                "tools": [
                    {
                        "name": "remote_echo",
                        "description": "Echoes back input payload from remote server",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "text": { "type": "string" }
                            }
                        }
                    }
                ]
            }
        });
        let body_str = serde_json::to_string(&resp_payload).unwrap();
        let http_resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body_str.len(),
            body_str
        );
        post_stream.write_all(http_resp.as_bytes()).await.unwrap();
    });

    let client_url = format!("http://{}", addr);
    let client =
        McpClient::connect_with_auth(&client_url, Some("secret-mcp-token".to_string()), None)
            .await
            .expect("Client should successfully connect to SSE");

    assert_eq!(client.transport_type(), "http/sse");

    // Wait for server to process endpoint event
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Call tools/list
    let tools = client
        .list_tools()
        .await
        .expect("tools/list should succeed");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].manifest().name, "remote_echo");
    assert_eq!(
        tools[0].manifest().description,
        "Echoes back input payload from remote server"
    );

    server_task.await.unwrap();
    let _ = client.close().await;
}

#[tokio::test]
async fn test_mcp_streamable_http_deferred_sse_response() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server_task = tokio::spawn(async move {
        // SSE Connection
        let (mut sse_stream, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 2048];
        let _ = sse_stream.read(&mut buf).await.unwrap();

        let sse_header = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\n\r\n";
        sse_stream.write_all(sse_header.as_bytes()).await.unwrap();

        // Accept POST
        let (mut post_stream, _) = listener.accept().await.unwrap();
        let mut post_buf = [0u8; 4096];
        let n = post_stream.read(&mut post_buf).await.unwrap();
        let post_text = String::from_utf8_lossy(&post_buf[..n]);

        let json_start = post_text.find('{').unwrap();
        let req_json: serde_json::Value = serde_json::from_str(&post_text[json_start..]).unwrap();
        let req_id = req_json.get("id").unwrap().as_u64().unwrap();

        // Respond to POST with 202 Accepted and empty body
        let http_202 = "HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        post_stream.write_all(http_202.as_bytes()).await.unwrap();

        // Brief delay, then send response via SSE stream
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let sse_msg = format!(
            "event: message\r\ndata: {{\"jsonrpc\":\"2.0\",\"id\":{},\"result\":{{\"status\":\"deferred_success\"}}}}\r\n\r\n",
            req_id
        );
        sse_stream.write_all(sse_msg.as_bytes()).await.unwrap();
    });

    let client_url = format!("http://{}", addr);
    let client = McpClient::connect(&client_url).await.unwrap();

    let res = client
        .rpc_call("test/deferred", None)
        .await
        .expect("Deferred RPC call should resolve via SSE");

    assert_eq!(
        res.get("result")
            .and_then(|r| r.get("status"))
            .and_then(|s| s.as_str()),
        Some("deferred_success")
    );

    server_task.await.unwrap();
    let _ = client.close().await;
}

#[tokio::test]
async fn test_mcp_streamable_http_reconnect_backoff() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx_sse, mut rx_sse) = tokio::sync::mpsc::channel::<tokio::net::TcpStream>(4);
    let (tx_post, mut rx_post) = tokio::sync::mpsc::channel::<tokio::net::TcpStream>(4);

    let _router_task = tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let mut buf = [0u8; 4];
            if stream.peek(&mut buf).await.is_ok() {
                if buf.starts_with(b"GET") {
                    let _ = tx_sse.send(stream).await;
                } else {
                    let _ = tx_post.send(stream).await;
                }
            }
        }
    });

    let server_task = tokio::spawn(async move {
        // Step 1: Accept initial SSE connection and complete handshake before closing
        let mut stream1 = rx_sse.recv().await.unwrap();
        let mut buf1 = [0u8; 1024];
        let _ = stream1.read(&mut buf1).await.unwrap();
        let sse_header = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\n\r\n";
        stream1.write_all(sse_header.as_bytes()).await.unwrap();
        stream1.write_all(b": keepalive\r\n\r\n").await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        drop(stream1);

        // Step 2: Accept reconnected SSE stream (after client 1s backoff)
        let mut stream2 = rx_sse.recv().await.unwrap();
        let mut buf2 = [0u8; 1024];
        let _ = stream2.read(&mut buf2).await.unwrap();
        let sse_header2 = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\n\r\n";
        stream2.write_all(sse_header2.as_bytes()).await.unwrap();

        // Step 3: Accept POST
        let mut post_stream = rx_post.recv().await.unwrap();
        let mut post_buf = [0u8; 4096];
        let n = post_stream.read(&mut post_buf).await.unwrap();
        let post_text = String::from_utf8_lossy(&post_buf[..n]);
        let json_start = post_text.find('{').unwrap();
        let req_json: serde_json::Value = serde_json::from_str(&post_text[json_start..]).unwrap();
        let req_id = req_json.get("id").unwrap().as_u64().unwrap();

        let http_202 = "HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        post_stream.write_all(http_202.as_bytes()).await.unwrap();

        // Write response over reconnected stream2
        let sse_msg = format!(
            "event: message\r\ndata: {{\"jsonrpc\":\"2.0\",\"id\":{},\"result\":{{\"reconnected\":true}}}}\r\n\r\n",
            req_id
        );
        stream2.write_all(sse_msg.as_bytes()).await.unwrap();

        // Keep stream2 alive until client finishes
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    });

    let client_url = format!("http://{}", addr);
    let client = McpClient::connect(&client_url).await.unwrap();

    // Give time for first stream to drop and reconnect (1s backoff)
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

    let res = client
        .rpc_call("test/reconnect", None)
        .await
        .expect("RPC should succeed on reconnected stream");

    assert_eq!(
        res.get("result")
            .and_then(|r| r.get("reconnected"))
            .and_then(|v| v.as_bool()),
        Some(true)
    );

    server_task.await.unwrap();
    let _ = client.close().await;
}
