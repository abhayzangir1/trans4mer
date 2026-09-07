use super::provider::{LlmProvider, LlmRequest, LlmResponse, LlmStream, LlmStreamChunk};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use std::time::Duration;
use trans4mers_domain::config::ModelConfig;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::token_usage::TokenMetrics;

pub struct AnthropicProvider {
    client: Client,
    api_key: String,
    default_endpoint: String,
}

impl AnthropicProvider {
    pub const ANTHROPIC_VERSION: &'static str = "2023-06-01";

    pub fn new(api_key: String, default_endpoint: Option<String>) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(300))
                .build()
                .unwrap_or_default(),
            api_key,
            default_endpoint: default_endpoint
                .unwrap_or_else(|| "https://api.anthropic.com".to_string()),
        }
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn name(&self) -> &'static str {
        "anthropic"
    }

    async fn generate(&self, request: &LlmRequest) -> Result<LlmResponse, Trans4mersError> {
        let endpoint = request
            .config
            .provider_endpoint
            .as_deref()
            .unwrap_or(&self.default_endpoint);
        let url = format!("{}/v1/messages", endpoint.trim_end_matches('/'));
        let timeout = request.config.timeout_secs.unwrap_or(120);

        let mut system_text = String::new();
        let mut messages = Vec::new();

        for m in &request.messages {
            if m.role == "system" {
                if !system_text.is_empty() {
                    system_text.push_str("\n\n");
                }
                system_text.push_str(&m.content);
            } else {
                messages.push(json!({
                    "role": if m.role == "assistant" { "assistant" } else { "user" },
                    "content": m.content,
                }));
            }
        }

        // Map unified tool schemas to Anthropic tools schema
        let mut tools = Vec::new();
        if let Some(unified_tools) = &request.tools {
            for t in unified_tools {
                if let Some(function) = t.get("function") {
                    tools.push(json!({
                        "name": function.get("name").and_then(|n| n.as_str()).unwrap_or(""),
                        "description": function.get("description").and_then(|d| d.as_str()).unwrap_or(""),
                        "input_schema": function.get("parameters").cloned().unwrap_or_else(|| json!({"type": "object", "properties": {}})),
                    }));
                } else if let Some(name) = t.get("name").and_then(|n| n.as_str()) {
                    tools.push(json!({
                        "name": name,
                        "description": t.get("description").and_then(|d| d.as_str()).unwrap_or(""),
                        "input_schema": t.get("input_schema").cloned().unwrap_or_else(|| json!({"type": "object", "properties": {}})),
                    }));
                }
            }
        }

        let mut body = json!({
            "model": request.config.model,
            "messages": messages,
            "max_tokens": request.config.max_output_tokens.unwrap_or(2048),
        });

        if !system_text.is_empty() {
            body["system"] = json!(system_text);
        }
        if !tools.is_empty() {
            body["tools"] = json!(tools);
        }
        if let Some(temp) = request.config.temperature {
            body["temperature"] = json!(temp);
        }

        let resp = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", Self::ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .timeout(Duration::from_secs(timeout))
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    Trans4mersError::LlmTimeout {
                        timeout_secs: timeout,
                    }
                } else {
                    Trans4mersError::LlmProvider {
                        provider: "anthropic".to_string(),
                        message: format!("HTTP error: {}", e),
                    }
                }
            })?;

        if !resp.status().is_success() {
            let error_text = resp.text().await.unwrap_or_default();
            return Err(Trans4mersError::LlmProvider {
                provider: "anthropic".to_string(),
                message: format!("API returned error: {}", error_text),
            });
        }

        let json_resp: serde_json::Value =
            resp.json()
                .await
                .map_err(|e| Trans4mersError::LlmProvider {
                    provider: "anthropic".to_string(),
                    message: format!("Invalid JSON response: {}", e),
                })?;

        let mut content = String::new();
        let mut parsed_tool_calls = Vec::new();

        if let Some(blocks) = json_resp.get("content").and_then(|c| c.as_array()) {
            for (i, block) in blocks.iter().enumerate() {
                match block.get("type").and_then(|t| t.as_str()).unwrap_or("") {
                    "text" => {
                        if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                            content.push_str(t);
                        }
                    }
                    "tool_use" => {
                        let id = block
                            .get("id")
                            .and_then(|id| id.as_str())
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| format!("call_{}", i));
                        let name = block
                            .get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or("")
                            .to_string();
                        let input = block.get("input").cloned().unwrap_or_else(|| json!({}));

                        parsed_tool_calls.push(json!({
                            "id": id,
                            "type": "function",
                            "function": {
                                "name": name,
                                "arguments": serde_json::to_string(&input).unwrap_or_else(|_| "{}".to_string())
                            }
                        }));
                    }
                    _ => {}
                }
            }
        }

        let token_metrics = json_resp
            .get("usage")
            .and_then(|u| u.as_object())
            .map(|u| {
                let input = u.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                let output = u.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                TokenMetrics {
                    prompt_tokens: input as u32,
                    completion_tokens: output as u32,
                    total_tokens: (input + output) as u32,
                }
            })
            .unwrap_or_default();

        Ok(LlmResponse {
            content,
            tool_calls: if parsed_tool_calls.is_empty() {
                None
            } else {
                Some(parsed_tool_calls)
            },
            metrics: token_metrics,
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
        let url = format!("{}/v1/messages", endpoint.trim_end_matches('/'));
        let timeout = request.config.timeout_secs.unwrap_or(120);

        let mut system_text = String::new();
        let mut messages = Vec::new();

        for m in &request.messages {
            if m.role == "system" {
                if !system_text.is_empty() {
                    system_text.push('\n');
                }
                system_text.push_str(&m.content);
            } else {
                let role = match m.role.as_str() {
                    "assistant" => "assistant",
                    _ => "user",
                };
                messages.push(json!({
                    "role": role,
                    "content": m.content
                }));
            }
        }

        if messages.is_empty() {
            messages.push(json!({
                "role": "user",
                "content": "Begin execution."
            }));
        }

        let mut payload = json!({
            "model": request.config.model,
            "messages": messages,
            "max_tokens": request.config.max_output_tokens.unwrap_or(4096),
            "stream": true
        });

        if !system_text.is_empty() {
            payload["system"] = json!(system_text);
        }

        if let Some(temp) = request.config.temperature {
            payload["temperature"] = json!(temp);
        }

        if let Some(tools) = &request.tools {
            let anthropic_tools: Vec<serde_json::Value> = tools
                .iter()
                .filter_map(|t| {
                    let func = t.get("function")?;
                    Some(json!({
                        "name": func.get("name")?,
                        "description": func.get("description").unwrap_or(&json!("")),
                        "input_schema": func.get("parameters").unwrap_or(&json!({"type": "object"}))
                    }))
                })
                .collect();
            if !anthropic_tools.is_empty() {
                payload["tools"] = json!(anthropic_tools);
            }
        }

        let resp = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", Self::ANTHROPIC_VERSION)
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
                        provider: "anthropic".to_string(),
                        message: format!("HTTP error: {}", e),
                    }
                }
            })?;

        if !resp.status().is_success() {
            let error_text = resp.text().await.unwrap_or_default();
            return Err(Trans4mersError::LlmProvider {
                provider: "anthropic".to_string(),
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

                        if let Some(data_payload) = line_str.strip_prefix("data:") {
                            let trimmed_data = data_payload.trim();
                            if let Ok(json_val) =
                                serde_json::from_str::<serde_json::Value>(trimmed_data)
                            {
                                let event_type =
                                    json_val.get("type").and_then(|t| t.as_str()).unwrap_or("");
                                match event_type {
                                    "content_block_delta" => {
                                        if let Some(text) = json_val
                                            .get("delta")
                                            .and_then(|d| d.get("text"))
                                            .and_then(|t| t.as_str())
                                            .filter(|t| !t.is_empty())
                                        {
                                            return Some((
                                                Ok(LlmStreamChunk::TextDelta(text.to_string())),
                                                (bs, buf, finished),
                                            ));
                                        }
                                    }
                                    "message_delta" => {
                                        if let Some(usage) = json_val.get("usage") {
                                            let output_tokens = usage
                                                .get("output_tokens")
                                                .and_then(|t| t.as_u64())
                                                .unwrap_or(0);
                                            let metrics = TokenMetrics {
                                                prompt_tokens: 0,
                                                completion_tokens: output_tokens as u32,
                                                total_tokens: output_tokens as u32,
                                            };
                                            return Some((
                                                Ok(LlmStreamChunk::Usage(metrics)),
                                                (bs, buf, finished),
                                            ));
                                        }
                                    }
                                    "message_stop" => {
                                        return Some((
                                            Ok(LlmStreamChunk::Finish {
                                                reason: Some("stop".to_string()),
                                            }),
                                            (bs, buf, true),
                                        ));
                                    }
                                    _ => {}
                                }
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
                                    provider: "anthropic".to_string(),
                                    message: format!("Stream read error: {}", e),
                                }),
                                (bs, buf, finished),
                            ));
                        }
                        None => {
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

    async fn embed(&self, _text: &str, _config: &ModelConfig) -> Result<Vec<f32>, Trans4mersError> {
        Err(Trans4mersError::LlmProvider {
            provider: "anthropic".to_string(),
            message: "Anthropic does not support native text embeddings. Please configure a secondary provider like 'ollama' (with 'nomic-embed-text') or 'openai' (with 'text-embedding-3-small') in your embedding settings.".to_string(),
        })
    }
}
