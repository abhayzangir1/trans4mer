use super::provider::{LlmProvider, LlmRequest, LlmResponse, LlmStream, LlmStreamChunk};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use std::time::Duration;
use trans4mers_domain::config::ModelConfig;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::token_usage::TokenMetrics;

pub struct OpenAiCompatProvider {
    client: Client,
    name: String,
    default_endpoint: String,
    api_key: String,
}

impl OpenAiCompatProvider {
    pub fn new(name: String, default_endpoint: String, api_key: String) -> Self {
        Self {
            client: Client::new(),
            name,
            default_endpoint,
            api_key,
        }
    }
}

#[async_trait]
impl LlmProvider for OpenAiCompatProvider {
    fn name(&self) -> &str {
        self.name.as_str()
    }

    async fn generate(&self, request: &LlmRequest) -> Result<LlmResponse, Trans4mersError> {
        let endpoint = request
            .config
            .provider_endpoint
            .as_deref()
            .unwrap_or(&self.default_endpoint);
        let url = format!("{}/chat/completions", endpoint);
        let timeout = request.config.timeout_secs.unwrap_or(120);

        let mut payload = json!({
            "model": request.config.model,
            "messages": request.messages,
            "temperature": request.config.temperature.unwrap_or(0.7),
            "max_tokens": request.config.max_output_tokens,
        });

        if let Some(tools) = &request.tools {
            payload["tools"] = json!(tools);
        }

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
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
                        provider: self.name.clone(),
                        message: format!("HTTP error: {}", e),
                    }
                }
            })?;

        if !resp.status().is_success() {
            let error_text = resp.text().await.unwrap_or_default();
            return Err(Trans4mersError::LlmProvider {
                provider: self.name.clone(),
                message: format!("API returned error: {}", error_text),
            });
        }

        let json_resp: serde_json::Value =
            resp.json()
                .await
                .map_err(|e| Trans4mersError::LlmProvider {
                    provider: self.name.clone(),
                    message: format!("Invalid JSON response: {}", e),
                })?;

        let choice = json_resp
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|c| c.first())
            .ok_or_else(|| Trans4mersError::LlmProvider {
                provider: self.name.clone(),
                message: "Missing 'choices[0]' in response".to_string(),
            })?;

        let message = choice
            .get("message")
            .and_then(|m| m.as_object())
            .ok_or_else(|| Trans4mersError::LlmProvider {
                provider: self.name.clone(),
                message: "Missing 'message' in choice".to_string(),
            })?;

        let content = message
            .get("content")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();

        let tool_calls = message
            .get("tool_calls")
            .and_then(|tc| tc.as_array())
            .cloned();

        let usage = json_resp.get("usage");
        let prompt_tokens = usage
            .and_then(|u| u.get("prompt_tokens"))
            .and_then(|c| c.as_u64())
            .unwrap_or(0) as u32;
        let completion_tokens = usage
            .and_then(|u| u.get("completion_tokens"))
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
        let url = format!("{}/chat/completions", endpoint);
        let timeout = request.config.timeout_secs.unwrap_or(120);

        let mut payload = json!({
            "model": request.config.model,
            "messages": request.messages,
            "temperature": request.config.temperature.unwrap_or(0.7),
            "max_tokens": request.config.max_output_tokens,
            "stream": true,
            "stream_options": { "include_usage": true }
        });

        if let Some(tools) = &request.tools {
            payload["tools"] = json!(tools);
        }

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
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
                        provider: self.name.clone(),
                        message: format!("HTTP error: {}", e),
                    }
                }
            })?;

        if !resp.status().is_success() {
            let error_text = resp.text().await.unwrap_or_default();
            return Err(Trans4mersError::LlmProvider {
                provider: self.name.clone(),
                message: format!("API returned error: {}", error_text),
            });
        }

        let byte_stream = resp.bytes_stream();
        let provider_name = self.name.clone();
        let stream = futures::stream::unfold(
            (byte_stream, Vec::<u8>::new(), false),
            move |(mut bs, mut buf, mut finished)| {
                let p_name = provider_name.clone();
                async move {
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
                                if trimmed_data == "[DONE]" {
                                    return Some((
                                        Ok(LlmStreamChunk::Finish {
                                            reason: Some("done".to_string()),
                                        }),
                                        (bs, buf, true),
                                    ));
                                }

                                if let Ok(json_val) =
                                    serde_json::from_str::<serde_json::Value>(trimmed_data)
                                {
                                    if let Some(usage) = json_val.get("usage") {
                                        let prompt_tokens = usage
                                            .get("prompt_tokens")
                                            .and_then(|p| p.as_u64())
                                            .unwrap_or(0);
                                        let completion_tokens = usage
                                            .get("completion_tokens")
                                            .and_then(|c| c.as_u64())
                                            .unwrap_or(0);
                                        let metrics = TokenMetrics {
                                            prompt_tokens: prompt_tokens as u32,
                                            completion_tokens: completion_tokens as u32,
                                            total_tokens: (prompt_tokens + completion_tokens)
                                                as u32,
                                        };
                                        return Some((
                                            Ok(LlmStreamChunk::Usage(metrics)),
                                            (bs, buf, finished),
                                        ));
                                    }

                                    if let Some(choice) = json_val
                                        .get("choices")
                                        .and_then(|c| c.as_array())
                                        .and_then(|c| c.first())
                                    {
                                        if let Some(content) = choice
                                            .get("delta")
                                            .and_then(|d| d.get("content"))
                                            .and_then(|c| c.as_str())
                                            .filter(|c| !c.is_empty())
                                        {
                                            return Some((
                                                Ok(LlmStreamChunk::TextDelta(content.to_string())),
                                                (bs, buf, finished),
                                            ));
                                        }

                                        if let Some(finish_reason) = choice
                                            .get("finish_reason")
                                            .and_then(|f| f.as_str())
                                            .filter(|f| *f == "stop" || *f == "tool_calls")
                                        {
                                            return Some((
                                                Ok(LlmStreamChunk::Finish {
                                                    reason: Some(finish_reason.to_string()),
                                                }),
                                                (bs, buf, true),
                                            ));
                                        }
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
                                        provider: p_name,
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
        let url = format!("{}/embeddings", endpoint);

        let payload = json!({
            "model": config.model,
            "input": text
        });

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&payload)
            .send()
            .await
            .map_err(|e| Trans4mersError::LlmProvider {
                provider: self.name.clone(),
                message: format!("HTTP error: {}", e),
            })?;

        let json_resp: serde_json::Value =
            resp.json()
                .await
                .map_err(|e| Trans4mersError::LlmProvider {
                    provider: self.name.clone(),
                    message: format!("Invalid JSON response: {}", e),
                })?;

        let embedding = json_resp
            .get("data")
            .and_then(|d| d.as_array())
            .and_then(|d| d.first())
            .and_then(|d| d.get("embedding"))
            .and_then(|e| e.as_array())
            .ok_or_else(|| Trans4mersError::LlmProvider {
                provider: self.name.clone(),
                message: "Missing 'data[0].embedding' array".to_string(),
            })?
            .iter()
            .filter_map(|v| v.as_f64().map(|f| f as f32))
            .collect::<Vec<f32>>();

        Ok(embedding)
    }
}
