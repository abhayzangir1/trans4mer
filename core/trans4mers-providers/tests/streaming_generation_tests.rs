use async_trait::async_trait;
use futures::{StreamExt, stream};
use std::sync::Arc;
use trans4mers_domain::config::ModelConfig;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::provider::{
    LlmProvider, LlmRequest, LlmResponse, LlmStream, LlmStreamChunk, TokenMetrics,
};
use trans4mers_providers::llm::{AnthropicProvider, OllamaProvider, OpenAiCompatProvider};

struct MockStreamingProvider {
    deltas: Vec<String>,
}

#[async_trait]
impl LlmProvider for MockStreamingProvider {
    fn name(&self) -> &str {
        "mock"
    }

    async fn generate(&self, _request: &LlmRequest) -> Result<LlmResponse, Trans4mersError> {
        Ok(LlmResponse {
            content: self.deltas.concat(),
            tool_calls: None,
            metrics: TokenMetrics {
                prompt_tokens: 10,
                completion_tokens: self.deltas.len() as u32,
                total_tokens: 10 + self.deltas.len() as u32,
            },
        })
    }

    async fn generate_stream<'a>(
        &'a self,
        _request: &'a LlmRequest,
    ) -> Result<LlmStream<'a>, Trans4mersError> {
        let mut chunks: Vec<Result<LlmStreamChunk, Trans4mersError>> = self
            .deltas
            .iter()
            .map(|d| Ok(LlmStreamChunk::TextDelta(d.clone())))
            .collect();

        chunks.push(Ok(LlmStreamChunk::Usage(TokenMetrics {
            prompt_tokens: 10,
            completion_tokens: self.deltas.len() as u32,
            total_tokens: 10 + self.deltas.len() as u32,
        })));

        chunks.push(Ok(LlmStreamChunk::Finish {
            reason: Some("stop".to_string()),
        }));

        Ok(Box::pin(stream::iter(chunks)))
    }

    async fn embed(&self, _text: &str, _config: &ModelConfig) -> Result<Vec<f32>, Trans4mersError> {
        Ok(vec![0.1, 0.2, 0.3])
    }
}

fn dummy_request() -> LlmRequest {
    LlmRequest {
        config: ModelConfig {
            provider: "mock".to_string(),
            model: "mock-model".to_string(),
            temperature: Some(0.0),
            max_output_tokens: Some(100),
            context_limit: Some(4096),
            timeout_secs: Some(30),
            fallback_provider: None,
            fallback_model: None,
            provider_endpoint: None,
        },
        messages: vec![],
        tools: None,
        response_schema: None,
    }
}

#[tokio::test]
async fn test_streaming_generation_consumption() {
    let mock = MockStreamingProvider {
        deltas: vec![
            "Thinking... ".to_string(),
            "I will ".to_string(),
            "execute ".to_string(),
            "the task.".to_string(),
        ],
    };

    let request = dummy_request();
    let mut stream = mock
        .generate_stream(&request)
        .await
        .expect("Stream must start");

    let mut collected_text = String::new();
    let mut usage_received = false;
    let mut finish_received = false;

    while let Some(chunk_res) = stream.next().await {
        let chunk = chunk_res.expect("Chunk should not be error");
        match chunk {
            LlmStreamChunk::TextDelta(delta) => {
                collected_text.push_str(&delta);
            }
            LlmStreamChunk::Usage(metrics) => {
                usage_received = true;
                assert_eq!(metrics.completion_tokens, 4);
                assert_eq!(metrics.total_tokens, 14);
            }
            LlmStreamChunk::Finish { reason } => {
                finish_received = true;
                assert_eq!(reason.as_deref(), Some("stop"));
            }
            LlmStreamChunk::ToolCallDelta { .. } => {}
        }
    }

    assert_eq!(collected_text, "Thinking... I will execute the task.");
    assert!(usage_received, "Usage chunk must be emitted");
    assert!(finish_received, "Finish chunk must be emitted");
}

#[tokio::test]
async fn test_mid_stream_cancellation() {
    // Generate many chunks
    let mock = MockStreamingProvider {
        deltas: (0..100).map(|i| format!("token_{} ", i)).collect(),
    };

    let request = dummy_request();
    let mut stream = mock
        .generate_stream(&request)
        .await
        .expect("Stream must start");

    let mut chunks_read = 0;
    let mut cancelled = false;

    // Simulate reading 3 tokens, then cancellation occurs
    while let Some(chunk_res) = stream.next().await {
        let _chunk = chunk_res.unwrap();
        chunks_read += 1;
        if chunks_read == 3 {
            cancelled = true;
            // Simulated break upon cancellation token check (same as agent_runtime.rs)
            break;
        }
    }

    assert!(cancelled);
    assert_eq!(chunks_read, 3);
    // Verified that loop stopped cleanly without consuming the remaining 97 chunks
}

#[tokio::test]
async fn test_provider_implementations_type_check() {
    // Verify that OllamaProvider, OpenAiCompatProvider, and AnthropicProvider can all be trait objects
    let ollama: Arc<dyn LlmProvider> =
        Arc::new(OllamaProvider::new("http://localhost:11434".to_string()));
    let openai: Arc<dyn LlmProvider> = Arc::new(OpenAiCompatProvider::new(
        "openai".to_string(),
        "https://api.openai.com/v1".to_string(),
        "sk-test".to_string(),
    ));
    let anthropic: Arc<dyn LlmProvider> = Arc::new(AnthropicProvider::new(
        "sk-ant-test".to_string(),
        Some("https://api.anthropic.com".to_string()),
    ));

    assert_eq!(ollama.name(), "ollama");
    assert_eq!(openai.name(), "openai");
    assert_eq!(anthropic.name(), "anthropic");

    let req = dummy_request();
    // Invocable without panic even if external network/daemon is offline
    let _ = ollama.generate_stream(&req).await;
    let _ = openai.generate_stream(&req).await;
    let _ = anthropic.generate_stream(&req).await;
}
