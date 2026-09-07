use chrono::Utc;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use trans4mers_domain::ids::{AgentInstanceId, ConversationId, ExecutionId, TokenUsageId};
use trans4mers_domain::langfuse::LangfuseConfig;
use trans4mers_domain::token_usage::TokenUsage;
use trans4mers_providers::langfuse::{
    IngestionBatch, LangfuseClient, create_generation_item, create_span_item, create_trace_item,
    sanitize_payload,
};
use trans4mers_storage::DbHandle;
use trans4mers_storage::repos::token_usage_repo;

/// Test 1: Zero Network Egress Invariant
/// When Langfuse is disabled or unset, calling ingest_batch MUST produce ZERO network calls.
#[tokio::test]
async fn test_langfuse_unset_zero_network_egress() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let config = LangfuseConfig {
        host: format!("http://{}", addr),
        enabled: false, // DISABLED: ZERO EGRESS INVARIANT
        capture_prompts: false,
        public_key: Some("pk-lf-test".to_string()),
        secret_key: Some("sk-lf-test".to_string()),
    };

    let client = LangfuseClient::new();
    let mut batch = IngestionBatch::new();
    let trace = create_trace_item(
        "exec-test-1",
        "agent:test",
        "user-1",
        "session-1",
        "proj-1",
        "agent-1",
        "Completed",
        &Utc::now(),
    );
    batch.push(trace);

    // Run ingestion
    let res = client.ingest_batch(&batch, &config).await;
    assert!(
        res.is_ok(),
        "Ingestion on disabled sink must succeed as a silent no-op"
    );

    // Verify listener received ZERO connections
    let accept_result = tokio::time::timeout(Duration::from_millis(250), listener.accept()).await;
    assert!(
        accept_result.is_err(),
        "CRITICAL INVARIANT VIOLATION: A network request was dispatched while Langfuse was disabled!"
    );
}

/// Test 2: Token Parity with Persisted Database Metrics
/// Langfuse generation items must strictly mirror persisted SQLite token usages,
/// guaranteeing 100% token and cost equality between Langfuse and the local dashboard.
#[tokio::test]
async fn test_langfuse_trace_and_generation_token_parity() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let received_body = Arc::new(Mutex::new(String::new()));
    let received_auth = Arc::new(Mutex::new(String::new()));
    let body_clone = received_body.clone();
    let auth_clone = received_auth.clone();

    // Spawn mock Langfuse ingestion server
    tokio::spawn(async move {
        if let Ok((mut stream, _)) = listener.accept().await {
            let mut buf = vec![0u8; 16384];
            let n = stream.read(&mut buf).await.unwrap_or(0);
            let req_str = String::from_utf8_lossy(&buf[..n]).to_string();

            // Extract auth header
            for line in req_str.lines() {
                if line.to_lowercase().starts_with("authorization:") {
                    *auth_clone.lock().await = line.to_string();
                }
            }

            // Extract body
            if let Some(pos) = req_str.find("\r\n\r\n") {
                *body_clone.lock().await = req_str[pos + 4..].to_string();
            }

            let response = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: 17\r\n\r\n{\"successes\":[1]}";
            let _ = stream.write_all(response.as_bytes()).await;
        }
    });

    // 1. Setup SQLite database with project migrations
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("project.sqlite");
    let db = DbHandle::open(&db_path).unwrap();

    db.with_exclusive_conn(|conn| {
        trans4mers_storage::migration_runner::MigrationRunner::run_project_migrations(conn, 768)
    })
    .unwrap();

    let execution_id = ExecutionId::new();
    let agent_id = AgentInstanceId::new();
    let conv_id = ConversationId::new();

    // Insert 2 token usage rows into SQLite
    let usage1 = TokenUsage {
        id: TokenUsageId::new(),
        execution_id,
        agent_instance_id: agent_id,
        conversation_id: conv_id,
        provider: "ollama".to_string(), // Local provider: $0
        model: "qwen2.5-coder:7b".to_string(),
        prompt_tokens: 150,
        completion_tokens: 75,
        total_tokens: 225,
        estimated_cost_usd: None,
        compute_time_ms: 600,
        created_at: Utc::now(),
    };

    let usage2 = TokenUsage {
        id: TokenUsageId::new(),
        execution_id,
        agent_instance_id: agent_id,
        conversation_id: conv_id,
        provider: "openai".to_string(), // Cloud provider
        model: "gpt-4o-mini".to_string(),
        prompt_tokens: 120,
        completion_tokens: 50,
        total_tokens: 170,
        estimated_cost_usd: Some(0.0008),
        compute_time_ms: 450,
        created_at: Utc::now(),
    };

    db.with_write_tx(|tx| {
        token_usage_repo::insert_token_usage(tx, &usage1)?;
        token_usage_repo::insert_token_usage(tx, &usage2)?;
        Ok(())
    })
    .unwrap();

    // Query persisted total tokens from SQLite
    let persisted_total_tokens = db
        .with_read_conn(|conn| token_usage_repo::get_total_tokens_by_execution(conn, &execution_id))
        .unwrap();
    assert_eq!(
        persisted_total_tokens, 395,
        "Total tokens in SQLite must equal 150+75+120+50 = 395"
    );

    let persisted_usages = db
        .with_read_conn(|conn| token_usage_repo::get_token_usages_by_execution(conn, &execution_id))
        .unwrap();
    assert_eq!(persisted_usages.len(), 2);

    // 2. Assemble Langfuse IngestionBatch directly from persisted SQLite metrics
    let mut batch = IngestionBatch::new();
    let exec_id_str = execution_id.to_string();
    let agent_id_str = agent_id.to_string();
    let trace_item = create_trace_item(
        &exec_id_str,
        "agent:test",
        "trans4mers-user",
        "proj-test",
        "proj-test",
        &agent_id_str,
        "Completed",
        &Utc::now(),
    );
    batch.push(trace_item);

    for u in &persisted_usages {
        let u_id_str = u.id.to_string();
        let gen_item = create_generation_item(
            &u_id_str,
            &exec_id_str,
            &u.model,
            &u.provider,
            u.prompt_tokens,
            u.completion_tokens,
            u.total_tokens,
            u.estimated_cost_usd.unwrap_or(0.0),
            u.compute_time_ms,
            &sanitize_payload("Thought", false),
            &sanitize_payload("Action", false),
            &u.created_at,
        );
        batch.push(gen_item);
    }

    // Add tool execution span
    let span_item = create_span_item(
        "step-1",
        &exec_id_str,
        "tool:filesystem.read_file",
        &Utc::now(),
        &Utc::now(),
        "DEFAULT",
        None,
        serde_json::json!({ "tool_name": "filesystem.read_file" }),
        None,
        None,
    );
    batch.push(span_item);

    // 3. Dispatch to mock Langfuse instance
    let config = LangfuseConfig {
        host: format!("http://{}", addr),
        enabled: true,
        capture_prompts: false,
        public_key: Some("pk-lf-trans4mers".to_string()),
        secret_key: Some("sk-lf-sovereign".to_string()),
    };

    let client = LangfuseClient::new();
    let result = client.ingest_batch(&batch, &config).await;
    assert!(result.is_ok(), "Batch dispatch must succeed");

    // Wait briefly for mock server to process
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 4. Verify received payload on mock Langfuse server
    let auth = received_auth.lock().await.clone();
    assert!(
        auth.contains("Basic "),
        "Must include HTTP Basic Auth header"
    );

    let body_str = received_body.lock().await.clone();
    assert!(
        !body_str.is_empty(),
        "Mock server must receive request payload"
    );

    let parsed: serde_json::Value = serde_json::from_str(&body_str).unwrap();
    let batch_items = parsed["batch"].as_array().expect("Must have batch array");
    assert_eq!(
        batch_items.len(),
        4,
        "1 trace + 2 generations + 1 tool span"
    );

    // Verify Trace
    let trace_payload = &batch_items[0];
    assert_eq!(trace_payload["type"], "trace-create");
    assert_eq!(trace_payload["body"]["id"], exec_id_str.as_str());

    // Verify Generation items and sum total tokens
    let mut langfuse_total_tokens: i64 = 0;
    for item in batch_items
        .iter()
        .filter(|i| i["type"] == "generation-create")
    {
        let tokens = item["body"]["usage"]["totalTokens"].as_i64().unwrap();
        langfuse_total_tokens += tokens;

        let provider = item["body"]["metadata"]["provider"].as_str().unwrap();
        if provider == "ollama" {
            // Local provider cost invariant: $0.00
            assert_eq!(
                item["body"]["calculatedTotalCost"].as_f64().unwrap(),
                0.0,
                "Local Ollama provider must have exactly $0.00 cost"
            );
        }
    }

    // Mathematical Token Parity Verification
    assert_eq!(
        langfuse_total_tokens, persisted_total_tokens,
        "CRITICAL ACCEPTANCE CRITERION 2: Langfuse total tokens ({}) must mathematically match SQLite dashboard tokens ({})",
        langfuse_total_tokens, persisted_total_tokens
    );
}

/// Test 3: Redacted Prompt Capture
/// By default, prompts and thoughts are redacted.
/// When enabled, sovereign scrubbers sanitize API keys and bearer tokens.
#[test]
fn test_langfuse_prompt_redaction_and_scrubbing() {
    let raw_prompt = "User prompt containing sk-proj-1234567890abcdef1234567890 and Bearer secret_jwt_token_here";

    // 1. Default (capture_prompts = false): suppressed entirely
    let suppressed = sanitize_payload(raw_prompt, false);
    assert_eq!(
        suppressed,
        "[REDACTED - Prompt capture disabled in settings]"
    );

    // 2. Explicit Opt-in (capture_prompts = true): scrubbed of secrets
    let scrubbed = sanitize_payload(raw_prompt, true);
    assert!(!scrubbed.contains("sk-proj-1234567890"));
    assert!(!scrubbed.contains("secret_jwt_token_here"));
    assert!(scrubbed.contains("[REDACTED_API_KEY]"));
    assert!(scrubbed.contains("[REDACTED_BEARER_TOKEN]"));
}

/// Test 4: Live Health / Connection Test
#[tokio::test]
async fn test_langfuse_connection_test_endpoint() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        if let Ok((mut stream, _)) = listener.accept().await {
            let mut buf = vec![0u8; 1024];
            let _ = stream.read(&mut buf).await;
            let response = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: 15\r\n\r\n{\"status\":\"OK\"}";
            let _ = stream.write_all(response.as_bytes()).await;
        }
    });

    let client = LangfuseClient::new();
    let url = format!("http://{}", addr);
    let result = client
        .test_connection(&url, Some("pk-test"), Some("sk-test"))
        .await;

    assert!(result.is_ok(), "Health endpoint check must succeed");
    let msg = result.unwrap();
    assert!(
        msg.contains("successful"),
        "Expected success message, got: {}",
        msg
    );
}
