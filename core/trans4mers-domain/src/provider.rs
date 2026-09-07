use crate::config::ModelConfig;
use crate::error::Trans4mersError;
pub use crate::token_usage::TokenMetrics;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRequest {
    pub messages: Vec<LlmMessage>,
    pub config: ModelConfig,
    pub tools: Option<Vec<serde_json::Value>>,
    #[serde(default)]
    pub response_schema: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmMessage {
    pub role: String,
    pub content: String,
}

use futures::Stream;
use std::pin::Pin;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum LlmStreamChunk {
    TextDelta(String),
    ToolCallDelta {
        index: usize,
        id: Option<String>,
        name: Option<String>,
        arguments_delta: String,
    },
    Usage(TokenMetrics),
    Finish {
        reason: Option<String>,
    },
}

pub type LlmStream<'a> =
    Pin<Box<dyn Stream<Item = Result<LlmStreamChunk, Trans4mersError>> + Send + 'a>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    pub content: String,
    pub tool_calls: Option<Vec<serde_json::Value>>,
    pub metrics: TokenMetrics,
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Returns the canonical name of this provider (e.g., "ollama", "openai", "anthropic", "openrouter")
    fn name(&self) -> &str;

    /// Sends a prompt to the LLM and returns the response.
    /// This is strictly stateless. Token persistence is handled by the caller.
    async fn generate(&self, request: &LlmRequest) -> Result<LlmResponse, Trans4mersError>;

    /// Streams tokens and events incrementally from the LLM.
    /// Yields TextDelta chunks, optional ToolCallDelta, Usage metrics, and Finish.
    async fn generate_stream<'a>(
        &'a self,
        request: &'a LlmRequest,
    ) -> Result<LlmStream<'a>, Trans4mersError> {
        let resp = self.generate(request).await?;
        let mut chunks = Vec::new();
        if !resp.content.is_empty() {
            chunks.push(Ok(LlmStreamChunk::TextDelta(resp.content)));
        }
        if let Some(tool_calls) = resp.tool_calls {
            for (idx, tc) in tool_calls.into_iter().enumerate() {
                let id = tc
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(ToString::to_string);
                let name = tc
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|v| v.as_str())
                    .map(ToString::to_string);
                let args = tc
                    .get("function")
                    .and_then(|f| f.get("arguments"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                chunks.push(Ok(LlmStreamChunk::ToolCallDelta {
                    index: idx,
                    id,
                    name,
                    arguments_delta: args,
                }));
            }
        }
        chunks.push(Ok(LlmStreamChunk::Usage(resp.metrics)));
        chunks.push(Ok(LlmStreamChunk::Finish {
            reason: Some("stop".to_string()),
        }));
        Ok(Box::pin(futures::stream::iter(chunks)))
    }

    /// Generates a vector embedding for the given text.
    async fn embed(&self, text: &str, config: &ModelConfig) -> Result<Vec<f32>, Trans4mersError>;
}

/// Registry of available LLM providers.
pub struct ProviderRegistry {
    providers: RwLock<HashMap<String, Arc<dyn LlmProvider>>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: RwLock::new(HashMap::new()),
        }
    }

    pub fn register(&self, provider: Arc<dyn LlmProvider>) {
        if let Ok(mut map) = self.providers.write() {
            map.insert(provider.name().to_string(), provider);
        }
    }

    pub fn get(&self, name: &str) -> Result<Arc<dyn LlmProvider>, Trans4mersError> {
        let map = self.providers.read().map_err(|e| {
            Trans4mersError::Internal(format!(
                "Failed to acquire read lock on provider registry: {}",
                e
            ))
        })?;
        map.get(name).cloned().ok_or_else(|| {
            Trans4mersError::Internal(format!("LLM Provider '{}' not found in registry", name))
        })
    }

    pub fn get_default(&self) -> Result<Arc<dyn LlmProvider>, Trans4mersError> {
        let map = self.providers.read().map_err(|e| {
            Trans4mersError::Internal(format!(
                "Failed to acquire read lock on provider registry: {}",
                e
            ))
        })?;
        if let Some(p) = map.get("ollama").cloned() {
            return Ok(p);
        }
        if let Some(p) = map.values().next().cloned() {
            return Ok(p);
        }
        Err(Trans4mersError::Internal(
            "No LLM providers registered in registry".to_string(),
        ))
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}
