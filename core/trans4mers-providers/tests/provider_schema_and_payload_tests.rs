use serde_json::json;
use trans4mers_domain::config::ModelConfig;
use trans4mers_domain::provider::{LlmMessage, LlmRequest, LlmStreamChunk, TokenMetrics};

#[test]
fn test_llm_request_creation_and_serialization() {
    let config = ModelConfig {
        provider: "ollama".to_string(),
        model: "llama3.1:8b".to_string(),
        temperature: Some(0.2),
        max_output_tokens: Some(2048),
        context_limit: Some(8192),
        timeout_secs: Some(30),
        fallback_provider: None,
        fallback_model: None,
        provider_endpoint: None,
    };

    let req = LlmRequest {
        messages: vec![
            LlmMessage {
                role: "system".to_string(),
                content: "You are a sovereign security auditor.".to_string(),
            },
            LlmMessage {
                role: "user".to_string(),
                content: "Audit this Rust function for panics.".to_string(),
            },
            LlmMessage {
                role: "assistant".to_string(),
                content: "Found 0 unhandled unwraps.".to_string(),
            },
        ],
        config,
        tools: Some(vec![]),
        response_schema: None,
    };

    let serialized = serde_json::to_string(&req);
    assert!(serialized.is_ok(), "LlmRequest must serialize cleanly to JSON");

    let val: serde_json::Value = serde_json::from_str(&serialized.unwrap()).unwrap();
    assert_eq!(val["messages"].as_array().unwrap().len(), 3);
    assert_eq!(val["config"]["provider"], "ollama");
    assert_eq!(val["config"]["model"], "llama3.1:8b");
    assert_eq!(val["config"]["temperature"], 0.2);
}

#[test]
fn test_llm_stream_chunk_serialization() {
    let text_delta = LlmStreamChunk::TextDelta("Hello world".to_string());
    let serialized = serde_json::to_string(&text_delta).unwrap();
    assert!(serialized.contains("TextDelta"));

    let tool_delta = LlmStreamChunk::ToolCallDelta {
        index: 0,
        id: Some("call_123".to_string()),
        name: Some("fs_read".to_string()),
        arguments_delta: "{\"path\":".to_string(),
    };
    let tool_ser = serde_json::to_string(&tool_delta).unwrap();
    assert!(tool_ser.contains("ToolCallDelta"));
    assert!(tool_ser.contains("fs_read"));

    let usage = LlmStreamChunk::Usage(TokenMetrics {
        prompt_tokens: 50,
        completion_tokens: 30,
        total_tokens: 80,
    });
    let usage_ser = serde_json::to_string(&usage).unwrap();
    assert!(usage_ser.contains("Usage"));
}

#[test]
fn test_openai_function_calling_payload_structure() {
    let tool_def = json!({
        "type": "function",
        "function": {
            "name": "fs_read",
            "description": "Read file contents",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }
        }
    });

    assert_eq!(tool_def["type"], "function");
    assert_eq!(tool_def["function"]["name"], "fs_read");
    assert_eq!(tool_def["function"]["parameters"]["required"][0], "path");
}

#[test]
fn test_anthropic_system_prompt_separation() {
    // Anthropic API v1 requires system prompt as a top-level parameter, NOT inside messages
    let system = "Act as an autonomous coding agent.";
    let messages = vec![
        json!({ "role": "user", "content": "Refactor error handling." }),
        json!({ "role": "assistant", "content": "Done." })
    ];

    let anthropic_body = json!({
        "model": "claude-3-5-sonnet-20241022",
        "system": system,
        "messages": messages,
        "max_tokens": 4096,
    });

    assert_eq!(anthropic_body["system"], system);
    assert_eq!(anthropic_body["messages"].as_array().unwrap().len(), 2);
}

#[test]
fn test_gemini_contents_parts_structure() {
    // Google Gemini API requires { contents: [ { role: "user", parts: [ { text: "..." } ] } ] }
    let gemini_body = json!({
        "contents": [
            {
                "role": "user",
                "parts": [
                    { "text": "Analyze system architecture." }
                ]
            }
        ]
    });

    let parts = &gemini_body["contents"][0]["parts"];
    assert_eq!(parts[0]["text"], "Analyze system architecture.");
}

#[test]
fn test_token_estimation_heuristic() {
    // Heuristic: ~4 characters per token for English text
    let prompt = "Trans4mers Sovereign Agent Operating System with Local Inference";
    let estimated_tokens = (prompt.len() + 3) / 4;
    assert!(estimated_tokens >= 10 && estimated_tokens <= 20);
}

#[test]
fn test_api_key_redaction_in_errors() {
    let raw_error = "HTTP 401 Unauthorized: Invalid API key sk-ant-api03-abcdef1234567890 supplied.";
    let sensitive_prefix = "sk-ant-";

    let redacted = if let Some(idx) = raw_error.find(sensitive_prefix) {
        let mut masked = raw_error.to_string();
        masked.replace_range(idx + 7..std::cmp::min(idx + 25, raw_error.len()), "****************");
        masked
    } else {
        raw_error.to_string()
    };

    assert!(!redacted.contains("abcdef1234567890"), "Redacted string must not contain raw secret");
    assert!(redacted.contains("****************"), "Redacted string must contain masking asterisks");
}

#[test]
fn test_ollama_chunk_stream_json_parsing() {
    let raw_chunk = r#"{"model":"llama3.1:8b","created_at":"2026-09-08T00:00:00Z","message":{"role":"assistant","content":"Thinking step by step"},"done":false}"#;
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(raw_chunk);

    assert!(parsed.is_ok());
    let val = parsed.unwrap();
    assert_eq!(val["message"]["content"], "Thinking step by step");
    assert_eq!(val["done"], false);
}

#[test]
fn test_ollama_terminal_done_chunk_token_metrics() {
    let final_chunk = r#"{"model":"llama3.1:8b","done":true,"prompt_eval_count":142,"eval_count":88,"total_duration":1250000000}"#;
    let val: serde_json::Value = serde_json::from_str(final_chunk).unwrap();

    assert_eq!(val["done"], true);
    assert_eq!(val["prompt_eval_count"], 142);
    assert_eq!(val["eval_count"], 88);
}
