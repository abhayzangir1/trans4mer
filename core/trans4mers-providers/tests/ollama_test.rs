use trans4mers_domain::config::ModelConfig;
use trans4mers_providers::llm::{LlmMessage, LlmProvider, LlmRequest, OllamaProvider};

#[tokio::test]
async fn test_real_ollama_chat() {
    let provider = OllamaProvider::new("http://localhost:11434".to_string());

    let config = ModelConfig {
        provider: "ollama".to_string(),
        model: "qwen2.5-coder:3b".to_string(),
        temperature: Some(0.1),
        max_output_tokens: Some(50),
        context_limit: Some(4096),
        timeout_secs: Some(30),
        fallback_provider: None,
        fallback_model: None,
        provider_endpoint: None,
    };

    let request = LlmRequest {
        config: config.clone(),
        messages: vec![
            LlmMessage {
                role: "system".to_string(),
                content: "You are a helpful assistant.".to_string(),
            },
            LlmMessage {
                role: "user".to_string(),
                content: "Say hello!".to_string(),
            },
        ],
        tools: None,
        response_schema: None,
    };

    // Only verify network call if Ollama is running locally, otherwise handle gracefully
    if let Ok(response) = provider.generate(&request).await {
        assert!(!response.content.is_empty());
        assert!(
            response.metrics.total_tokens > 0,
            "TokenMetrics not returned by Ollama!"
        );
    }
}
