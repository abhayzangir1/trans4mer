use super::provider::{LlmProvider, LlmRequest, LlmResponse, LlmStream, LlmStreamChunk};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use std::time::Duration;
use trans4mers_domain::config::ModelConfig;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::token_usage::TokenMetrics;

pub struct OllamaProvider {
    client: Client,
    default_endpoint: String,
}

impl OllamaProvider {
    pub fn new(default_endpoint: String) -> Self {
        Self {
            client: Client::new(),
            default_endpoint,
        }
    }
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    fn name(&self) -> &'static str {
        "ollama"
    }

    async fn generate(&self, request: &LlmRequest) -> Result<LlmResponse, Trans4mersError> {
        let endpoint = request
            .config
            .provider_endpoint
            .as_deref()
            .unwrap_or(&self.default_endpoint);
        let url = format!("{}/api/chat", endpoint);

        let timeout = request.config.timeout_secs.unwrap_or(120);

        let mut payload = json!({
            "model": request.config.model,
            "messages": request.messages,
            "stream": false,
            "keep_alive": "30m",
            "options": {
                "temperature": request.config.temperature.unwrap_or(0.7),
            }
        });

        if let Some(max_tokens) = request.config.max_output_tokens {
            payload["options"]["num_predict"] = json!(max_tokens);
        }

        if let Some(tools) = request.tools.as_ref().filter(|t| !t.is_empty()) {
            payload["tools"] = json!(tools);
        }

        if let Some(schema) = &request.response_schema {
            payload["format"] = schema.clone();
        }

        let resp = self
            .client
            .post(&url)
            .timeout(Duration::from_secs(timeout))
            .json(&payload)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    Trans4mersError::LlmTimeout {
                        timeout_secs: timeout,
                    }
                } else {
                    Trans4mersError::LlmProvider {
                        provider: "ollama".to_string(),
                        message: format!("HTTP error: {}", e),
                    }
                }
            })?;

        if !resp.status().is_success() {
            let error_text = resp.text().await.unwrap_or_default();
            return Err(Trans4mersError::LlmProvider {
                provider: "ollama".to_string(),
                message: format!("API returned error: {}", error_text),
            });
        }

        let json_resp: serde_json::Value =
            resp.json()
                .await
                .map_err(|e| Trans4mersError::LlmProvider {
                    provider: "ollama".to_string(),
                    message: format!("Invalid JSON response: {}", e),
                })?;

        let message = json_resp
            .get("message")
            .and_then(|m| m.as_object())
            .ok_or_else(|| Trans4mersError::LlmProvider {
                provider: "ollama".to_string(),
                message: "Missing 'message' field in response".to_string(),
            })?;

        let content = message
            .get("content")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();

        // Extract native tool calls if present
        let tool_calls = message
            .get("tool_calls")
            .and_then(|tc| tc.as_array())
            .cloned();

        let prompt_tokens = json_resp
            .get("prompt_eval_count")
            .and_then(|c| c.as_u64())
            .unwrap_or(0) as u32;
        let completion_tokens = json_resp
            .get("eval_count")
            .and_then(|c| c.as_u64())
            .unwrap_or(0) as u32;

        Ok(LlmResponse {
            content,
            tool_calls,
            metrics: TokenMetrics {
                prompt_tokens,
                completion_tokens,
                total_tokens: prompt_tokens + completion_tokens,
            },
        })
    }

    async fn generate_stream<'a>(
        &'a self,
        request: &'a LlmRequest,
    ) -> Result<LlmStream<'a>, Trans4mersError> {
        let endpoint = request
            .config
            .provider_endpoint
            .as_deref()
            .unwrap_or(&self.default_endpoint);
        let url = format!("{}/api/chat", endpoint);
        let timeout = request.config.timeout_secs.unwrap_or(120);

        let mut payload = json!({
            "model": request.config.model,
            "messages": request.messages,
            "stream": true,
            "keep_alive": "30m",
            "options": {
                "temperature": request.config.temperature.unwrap_or(0.7),
            }
        });

        if let Some(max_tokens) = request.config.max_output_tokens {
            payload["options"]["num_predict"] = json!(max_tokens);
        }

        if let Some(tools) = request.tools.as_ref().filter(|t| !t.is_empty()) {
            payload["tools"] = json!(tools);
        }

        if let Some(schema) = &request.response_schema {
            payload["format"] = schema.clone();
        }

        let resp = self
            .client
            .post(&url)
            .timeout(Duration::from_secs(timeout))
            .json(&payload)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    Trans4mersError::LlmTimeout {
                        timeout_secs: timeout,
                    }
                } else {
                    Trans4mersError::LlmProvider {
                        provider: "ollama".to_string(),
                        message: format!("HTTP error: {}", e),
                    }
                }
            })?;

        if !resp.status().is_success() {
            let error_text = resp.text().await.unwrap_or_default();
            return Err(Trans4mersError::LlmProvider {
                provider: "ollama".to_string(),
                message: format!("API returned error: {}", error_text),
            });
        }

        let byte_stream = resp.bytes_stream();
        let stream = futures::stream::unfold(
            (byte_stream, Vec::<u8>::new(), false),
            move |(mut bs, mut buf, mut finished)| async move {
                if finished {
                    return None;
                }

                use futures::StreamExt;
                loop {
                    if let Some(newline_pos) = buf.iter().position(|&b| b == b'\n') {
                        let line_bytes = buf.drain(..=newline_pos).collect::<Vec<u8>>();
                        let line_str = String::from_utf8_lossy(&line_bytes).trim().to_string();
                        if line_str.is_empty() {
                            continue;
                        }

                        if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&line_str) {
                            let is_done = json_val
                                .get("done")
                                .and_then(|d| d.as_bool())
                                .unwrap_or(false);
                            let content_delta = json_val
                                .get("message")
                                .and_then(|m| m.get("content"))
                                .and_then(|c| c.as_str())
                                .unwrap_or("");

                            if is_done {
                                finished = true;
                                let prompt_tokens = json_val
                                    .get("prompt_eval_count")
                                    .and_then(|c| c.as_u64())
                                    .unwrap_or(0);
                                let completion_tokens = json_val
                                    .get("eval_count")
                                    .and_then(|c| c.as_u64())
                                    .unwrap_or(0);
                                let metrics = TokenMetrics {
                                    prompt_tokens: prompt_tokens as u32,
                                    completion_tokens: completion_tokens as u32,
                                    total_tokens: (prompt_tokens + completion_tokens) as u32,
                                };
                                return Some((
                                    Ok(LlmStreamChunk::Usage(metrics)),
                                    (bs, buf, finished),
                                ));
                            } else if let Some(tool_calls) = json_val
                                .get("message")
                                .and_then(|m| m.get("tool_calls"))
                                .and_then(|tc| tc.as_array())
                                .filter(|a| !a.is_empty())
                            {
                                if let Some(tc) = tool_calls.first() {
                                    let id = tc.get("id").and_then(|v| v.as_str()).map(String::from);
                                    let name = tc
                                        .get("function")
                                        .and_then(|f| f.get("name"))
                                        .and_then(|v| v.as_str())
                                        .map(String::from);
                                    let args = tc
                                        .get("function")
                                        .and_then(|f| f.get("arguments"))
                                        .map(|v| {
                                            if let Some(s) = v.as_str() {
                                                s.to_string()
                                            } else {
                                                v.to_string()
                                            }
                                        })
                                        .unwrap_or_default();
                                    return Some((
                                        Ok(LlmStreamChunk::ToolCallDelta {
                                            index: 0,
                                            id,
                                            name,
                                            arguments_delta: args,
                                        }),
                                        (bs, buf, finished),
                                    ));
                                }
                            } else if !content_delta.is_empty() {
                                return Some((
                                    Ok(LlmStreamChunk::TextDelta(content_delta.to_string())),
                                    (bs, buf, finished),
                                ));
                            }
                        }
                    }

                    match bs.next().await {
                        Some(Ok(bytes)) => {
                            buf.extend_from_slice(&bytes);
                        }
                        Some(Err(e)) => {
                            finished = true;
                            return Some((
                                Err(Trans4mersError::LlmProvider {
                                    provider: "ollama".to_string(),
                                    message: format!("Stream read error: {}", e),
                                }),
                                (bs, buf, finished),
                            ));
                        }
                        None => {
                            if !buf.is_empty() {
                                let line_str = String::from_utf8_lossy(&buf).trim().to_string();
                                if let Ok(json_val) =
                                    serde_json::from_str::<serde_json::Value>(&line_str)
                                {
                                    if let Some(tool_calls) = json_val
                                        .get("message")
                                        .and_then(|m| m.get("tool_calls"))
                                        .and_then(|tc| tc.as_array())
                                        .filter(|a| !a.is_empty())
                                    {
                                        if let Some(tc) = tool_calls.first() {
                                            let id = tc.get("id").and_then(|v| v.as_str()).map(String::from);
                                            let name = tc
                                                .get("function")
                                                .and_then(|f| f.get("name"))
                                                .and_then(|v| v.as_str())
                                                .map(String::from);
                                            let args = tc
                                                .get("function")
                                                .and_then(|f| f.get("arguments"))
                                                .map(|v| {
                                                    if let Some(s) = v.as_str() {
                                                        s.to_string()
                                                    } else {
                                                        v.to_string()
                                                    }
                                                })
                                                .unwrap_or_default();
                                            return Some((
                                                Ok(LlmStreamChunk::ToolCallDelta {
                                                    index: 0,
                                                    id,
                                                    name,
                                                    arguments_delta: args,
                                                }),
                                                (bs, Vec::new(), true),
                                            ));
                                        }
                                    }

                                    let content_delta = json_val
                                        .get("message")
                                        .and_then(|m| m.get("content"))
                                        .and_then(|c| c.as_str())
                                        .unwrap_or("");
                                    if !content_delta.is_empty() {
                                        return Some((
                                            Ok(LlmStreamChunk::TextDelta(
                                                content_delta.to_string(),
                                            )),
                                            (bs, Vec::new(), true),
                                        ));
                                    }
                                }
                            }
                            return Some((
                                Ok(LlmStreamChunk::Finish {
                                    reason: Some("done".to_string()),
                                }),
                                (bs, buf, true),
                            ));
                        }
                    }
                }
            },
        );

        Ok(Box::pin(stream))
    }

    async fn embed(&self, text: &str, config: &ModelConfig) -> Result<Vec<f32>, Trans4mersError> {
        let endpoint = config
            .provider_endpoint
            .as_deref()
            .unwrap_or(&self.default_endpoint);
        let url = format!("{}/api/embeddings", endpoint);

        let payload = json!({
            "model": config.model,
            "prompt": text
        });

        let resp = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| Trans4mersError::LlmProvider {
                provider: "ollama".to_string(),
                message: format!("HTTP error: {}", e),
            })?;

        let json_resp: serde_json::Value =
            resp.json()
                .await
                .map_err(|e| Trans4mersError::LlmProvider {
                    provider: "ollama".to_string(),
                    message: format!("Invalid JSON response: {}", e),
                })?;

        let embedding = json_resp
            .get("embedding")
            .and_then(|e| e.as_array())
            .ok_or_else(|| Trans4mersError::LlmProvider {
                provider: "ollama".to_string(),
                message: "Missing 'embedding' array".to_string(),
            })?
            .iter()
            .filter_map(|v| v.as_f64().map(|f| f as f32))
            .collect::<Vec<f32>>();

        Ok(embedding)
    }
}

/// Offline and loopback-only detector for local Ollama models.
/// Strict zero egress: does not make any external network requests or download binaries/models.
pub struct OllamaModelDetector;

impl OllamaModelDetector {
    /// Inspects the local filesystem on disk for installed Ollama models without opening any network sockets.
    /// Returns model names found in ~/.ollama/models/manifests/registry.ollama.ai/library/
    pub fn scan_local_disk_models() -> Vec<String> {
        let mut models = Vec::new();

        let base_dir = if cfg!(windows) {
            std::env::var("USERPROFILE").ok().map(|p| {
                std::path::PathBuf::from(p)
                    .join(".ollama")
                    .join("models")
                    .join("manifests")
            })
        } else {
            std::env::var("HOME").ok().map(|p| {
                std::path::PathBuf::from(p)
                    .join(".ollama")
                    .join("models")
                    .join("manifests")
            })
        };

        if let Some(manifest_root) = base_dir {
            let library_dir = manifest_root.join("registry.ollama.ai").join("library");
            if library_dir.is_dir()
                && let Ok(entries) = std::fs::read_dir(&library_dir)
            {
                for entry in entries.flatten() {
                    if entry.path().is_dir() {
                        let model_name = entry.file_name().to_string_lossy().to_string();
                        models.push(model_name);
                    }
                }
            }
        }

        models.sort();
        models
    }

    /// Checks if a known embedding model is present on disk.
    pub fn has_local_embedding_model() -> bool {
        let models = Self::scan_local_disk_models();
        models.iter().any(|m| {
            let lower = m.to_lowercase();
            lower.contains("embed") || lower.contains("bge") || lower.contains("minilm")
        })
    }

    /// Lazily checks local loopback (127.0.0.1:11434) with a bounded 500ms timeout.
    /// Strictly restricted to loopback addresses; returns Trans4mersError::PolicyDenied for external hosts.
    pub async fn check_loopback_endpoint(endpoint: &str) -> Result<Vec<String>, Trans4mersError> {
        let is_loopback = endpoint.contains("127.0.0.1") || endpoint.contains("localhost");
        if !is_loopback {
            return Err(Trans4mersError::PolicyDenied {
                capability: "loopback_only".to_string(),
            });
        }

        let clean_base = endpoint.trim_end_matches('/');
        let tags_url = format!("{}/api/tags", clean_base);

        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(500))
            .build()
            .map_err(|e| Trans4mersError::Internal(e.to_string()))?;

        let resp =
            client
                .get(&tags_url)
                .send()
                .await
                .map_err(|e| Trans4mersError::LlmProvider {
                    provider: "ollama".to_string(),
                    message: format!("Loopback unreachable: {}", e),
                })?;

        if !resp.status().is_success() {
            return Ok(Vec::new());
        }

        let json: serde_json::Value =
            resp.json()
                .await
                .map_err(|e| Trans4mersError::LlmProvider {
                    provider: "ollama".to_string(),
                    message: e.to_string(),
                })?;

        let mut names = Vec::new();
        if let Some(arr) = json.get("models").and_then(|m| m.as_array()) {
            for m in arr {
                if let Some(name) = m.get("name").and_then(|n| n.as_str()) {
                    names.push(name.to_string());
                }
            }
        }
        names.sort();
        Ok(names)
    }
}
