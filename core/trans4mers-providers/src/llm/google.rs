use super::provider::{LlmProvider, LlmRequest, LlmResponse};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use std::time::Duration;
use trans4mers_domain::config::ModelConfig;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::token_usage::TokenMetrics;

pub struct GoogleProvider {
    client: Client,
    api_key: String,
    default_endpoint: String,
}

impl GoogleProvider {
    pub fn new(api_key: String, default_endpoint: Option<String>) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(300))
                .build()
                .unwrap_or_default(),
            api_key,
            default_endpoint: default_endpoint
                .unwrap_or_else(|| "https://generativelanguage.googleapis.com".to_string()),
        }
    }
}

#[async_trait]
impl LlmProvider for GoogleProvider {
    fn name(&self) -> &'static str {
        "google"
    }

    async fn generate(&self, request: &LlmRequest) -> Result<LlmResponse, Trans4mersError> {
        let endpoint = request
            .config
            .provider_endpoint
            .as_deref()
            .unwrap_or(&self.default_endpoint);
        let model = &request.config.model;
        let url = format!(
            "{}/v1beta/models/{}:generateContent?key={}",
            endpoint.trim_end_matches('/'),
            model,
            self.api_key
        );
        let timeout = request.config.timeout_secs.unwrap_or(120);

        let mut system_text = String::new();
        let mut contents = Vec::new();

        for m in &request.messages {
            if m.role == "system" {
                if !system_text.is_empty() {
                    system_text.push_str("\n\n");
                }
                system_text.push_str(&m.content);
            } else {
                contents.push(json!({
                    "role": if m.role == "assistant" { "model" } else { "user" },
                    "parts": [{"text": m.content}]
                }));
            }
        }

        let mut body = json!({
            "contents": contents,
            "generationConfig": {
                "temperature": request.config.temperature.unwrap_or(0.7),
            }
        });

        if let Some(max_tokens) = request.config.max_output_tokens {
            body["generationConfig"]["maxOutputTokens"] = json!(max_tokens);
        }

        if !system_text.is_empty() {
            body["system_instruction"] = json!({
                "parts": [{"text": system_text}]
            });
        }

        // Map tools to Gemini functionDeclarations
        if let Some(tools) = &request.tools {
            let mut declarations = Vec::new();
            for t in tools {
                if let Some(function) = t.get("function") {
                    declarations.push(json!({
                        "name": function.get("name").and_then(|n| n.as_str()).unwrap_or(""),
                        "description": function.get("description").and_then(|d| d.as_str()).unwrap_or(""),
                        "parameters": function.get("parameters").cloned().unwrap_or_else(|| json!({"type": "object", "properties": {}})),
                    }));
                } else if let Some(name) = t.get("name").and_then(|n| n.as_str()) {
                    declarations.push(json!({
                        "name": name,
                        "description": t.get("description").and_then(|d| d.as_str()).unwrap_or(""),
                        "parameters": t.get("input_schema").cloned().unwrap_or_else(|| json!({"type": "object", "properties": {}})),
                    }));
                }
            }
            if !declarations.is_empty() {
                body["tools"] = json!([{"functionDeclarations": declarations}]);
            }
        }

        let resp = self
            .client
            .post(&url)
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
                        provider: "google".to_string(),
                        message: format!("HTTP error: {}", e),
                    }
                }
            })?;

        if !resp.status().is_success() {
            let error_text = resp.text().await.unwrap_or_default();
            return Err(Trans4mersError::LlmProvider {
                provider: "google".to_string(),
                message: format!("API returned error: {}", error_text),
            });
        }

        let json_resp: serde_json::Value =
            resp.json()
                .await
                .map_err(|e| Trans4mersError::LlmProvider {
                    provider: "google".to_string(),
                    message: format!("Invalid JSON response: {}", e),
                })?;

        let mut content = String::new();
        let mut parsed_tool_calls = Vec::new();

        let parts_opt = json_resp
            .get("candidates")
            .and_then(|c| c.as_array())
            .and_then(|candidates| candidates.first())
            .and_then(|candidate| candidate.get("content"))
            .and_then(|content| content.get("parts"))
            .and_then(|parts| parts.as_array());

        if let Some(parts) = parts_opt {
            for (i, part) in parts.iter().enumerate() {
                if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                    content.push_str(text);
                }
                if let Some(func_call) = part.get("functionCall") {
                    let name = func_call
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("")
                        .to_string();
                    let args = func_call.get("args").cloned().unwrap_or_else(|| json!({}));
                    parsed_tool_calls.push(json!({
                        "id": format!("gcall_{}", i),
                        "type": "function",
                        "function": {
                            "name": name,
                            "arguments": serde_json::to_string(&args).unwrap_or_else(|_| "{}".to_string())
                        }
                    }));
                }
            }
        }

        let token_metrics = json_resp
            .get("usageMetadata")
            .and_then(|u| u.as_object())
            .map(|u| {
                let input = u
                    .get("promptTokenCount")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let output = u
                    .get("candidatesTokenCount")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
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

    async fn embed(&self, text: &str, config: &ModelConfig) -> Result<Vec<f32>, Trans4mersError> {
        let endpoint = config
            .provider_endpoint
            .as_deref()
            .unwrap_or(&self.default_endpoint);
        let model = if config.model.is_empty() || !config.model.contains("embed") {
            "text-embedding-004"
        } else {
            &config.model
        };
        let url = format!(
            "{}/v1beta/models/{}:embedContent?key={}",
            endpoint.trim_end_matches('/'),
            model,
            self.api_key
        );

        let body = json!({
            "content": {
                "parts": [{"text": text}]
            }
        });

        let resp = self
            .client
            .post(&url)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| Trans4mersError::LlmProvider {
                provider: "google".to_string(),
                message: format!("HTTP error: {}", e),
            })?;

        if !resp.status().is_success() {
            return Err(Trans4mersError::LlmProvider {
                provider: "google".to_string(),
                message: format!("Google embed error status: {}", resp.status()),
            });
        }

        let json_resp: serde_json::Value =
            resp.json()
                .await
                .map_err(|e| Trans4mersError::LlmProvider {
                    provider: "google".to_string(),
                    message: format!("Invalid JSON response: {}", e),
                })?;

        let embedding = json_resp
            .get("embedding")
            .and_then(|e| e.get("values"))
            .and_then(|v| v.as_array())
            .ok_or_else(|| Trans4mersError::LlmProvider {
                provider: "google".to_string(),
                message: "Missing 'embedding.values' in response".to_string(),
            })?
            .iter()
            .filter_map(|v| v.as_f64().map(|f| f as f32))
            .collect();

        Ok(embedding)
    }
}
