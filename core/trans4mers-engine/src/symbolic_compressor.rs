use serde::{Deserialize, Serialize};
use std::sync::Arc;
use trans4mers_domain::config::ModelConfig;
use trans4mers_domain::provider::{LlmMessage, LlmProvider, LlmRequest};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolicFact {
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub confidence: f32,
}

pub struct SymbolicCompressor;

impl SymbolicCompressor {
    /// Pure heuristic rule-based extraction that extracts factual statements from logs, tool outputs,
    /// and messages without burning LLM tokens. Essential for Low-Spec Local Mode.
    pub fn compress_heuristic(raw_text: &str) -> Vec<SymbolicFact> {
        let mut facts = Vec::new();

        for line in raw_text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            // 1. Key-Value or Config assignment: "key = value" or "key: value"
            if let Some((k, v)) = trimmed.split_once('=') {
                let key = k.trim();
                let val = v.trim();
                if !key.is_empty() && !val.is_empty() && key.len() < 50 && val.len() < 120 {
                    facts.push(SymbolicFact {
                        subject: key.to_string(),
                        predicate: "equals".to_string(),
                        object: val.to_string(),
                        confidence: 0.9,
                    });
                }
            } else if let Some((k, v)) = trimmed.split_once(':') {
                let key = k.trim();
                let val = v.trim();
                if !key.is_empty()
                    && !val.is_empty()
                    && !key.starts_with("http")
                    && key.len() < 40
                    && val.len() < 120
                {
                    facts.push(SymbolicFact {
                        subject: key.to_string(),
                        predicate: "is".to_string(),
                        object: val.to_string(),
                        confidence: 0.85,
                    });
                }
            }

            // 2. Error detection: "error[E0000]: msg" or "Error: msg"
            if trimmed.to_lowercase().contains("error") || trimmed.to_lowercase().contains("failed")
            {
                facts.push(SymbolicFact {
                    subject: "Execution".to_string(),
                    predicate: "encountered_error".to_string(),
                    object: trimmed.chars().take(150).collect(),
                    confidence: 0.95,
                });
            }

            // 3. File actions: "Wrote to path" or "Created file path"
            if trimmed.starts_with("Wrote to ") || trimmed.starts_with("Created ") {
                facts.push(SymbolicFact {
                    subject: "Filesystem".to_string(),
                    predicate: "modified_file".to_string(),
                    object: trimmed.to_string(),
                    confidence: 1.0,
                });
            }
        }

        facts
    }

    /// High-fidelity LLM-powered extraction that distills unstructured conversations into semantic predicates.
    pub async fn compress_with_provider(
        raw_text: &str,
        provider: Arc<dyn LlmProvider>,
        model_config: &ModelConfig,
    ) -> Vec<SymbolicFact> {
        let system_prompt = "You are a symbolic fact extraction engine. Convert the following text into a JSON array of concise predicate triples: [{\"subject\": \"...\", \"predicate\": \"...\", \"object\": \"...\", \"confidence\": 0.9}]. Output ONLY the valid JSON array.";
        let request = LlmRequest {
            messages: vec![
                LlmMessage {
                    role: "system".to_string(),
                    content: system_prompt.to_string(),
                },
                LlmMessage {
                    role: "user".to_string(),
                    content: raw_text.to_string(),
                },
            ],
            config: model_config.clone(),
            tools: None,
            response_schema: Some(serde_json::json!({
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "subject": { "type": "string" },
                        "predicate": { "type": "string" },
                        "object": { "type": "string" },
                        "confidence": { "type": "number" }
                    },
                    "required": ["subject", "predicate", "object"]
                }
            })),
        };

        if let Ok(resp) = provider.generate(&request).await {
            let text = resp.content.trim();
            if let Some(start) = text.find('[')
                && let Some(end) = text.rfind(']')
            {
                let json_slice = &text[start..=end];
                if let Ok(parsed) = serde_json::from_str::<Vec<SymbolicFact>>(json_slice) {
                    return parsed;
                }
            }
        }

        // Fallback to heuristic if LLM call fails or returns non-JSON
        Self::compress_heuristic(raw_text)
    }

    /// Formats an array of symbolic facts into condensed markdown for context injection.
    pub fn format_markdown(facts: &[SymbolicFact]) -> String {
        if facts.is_empty() {
            return String::new();
        }

        let mut out = String::from("### Symbolic Memory Facts\n");
        for f in facts {
            out.push_str(&format!(
                "- **{}** {} `{}`\n",
                f.subject, f.predicate, f.object
            ));
        }
        out
    }
}
