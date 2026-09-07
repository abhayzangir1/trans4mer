use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionBatch {
    pub batch: Vec<IngestionItem>,
}

impl IngestionBatch {
    pub fn new() -> Self {
        Self { batch: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.batch.is_empty()
    }

    pub fn len(&self) -> usize {
        self.batch.len()
    }

    pub fn push(&mut self, item: IngestionItem) {
        self.batch.push(item);
    }
}

impl Default for IngestionBatch {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionItem {
    pub id: String,
    #[serde(rename = "type")]
    pub item_type: String,
    pub timestamp: String,
    pub body: serde_json::Value,
}

#[allow(clippy::too_many_arguments)]
pub fn create_trace_item(
    trace_id: &str,
    name: &str,
    user_id: &str,
    session_id: &str,
    project_id: &str,
    agent_id: &str,
    status: &str,
    timestamp: &DateTime<Utc>,
) -> IngestionItem {
    let body = serde_json::json!({
        "id": trace_id,
        "name": name,
        "userId": user_id,
        "sessionId": session_id,
        "release": "0.1.0",
        "version": "0.1.0",
        "metadata": {
            "execution_id": trace_id,
            "project_id": project_id,
            "agent_instance_id": agent_id,
            "status": status,
        },
        "tags": ["trans4mers", "sovereign-agent"]
    });

    IngestionItem {
        id: Uuid::new_v4().to_string(),
        item_type: "trace-create".to_string(),
        timestamp: timestamp.to_rfc3339(),
        body,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn create_generation_item(
    generation_id: &str,
    trace_id: &str,
    model: &str,
    provider: &str,
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
    cost_usd: f64,
    compute_time_ms: u64,
    input_text: &str,
    output_text: &str,
    created_at: &DateTime<Utc>,
) -> IngestionItem {
    let start_time = *created_at - chrono::Duration::milliseconds(compute_time_ms as i64);
    let end_time = *created_at;

    // Ollama and local providers remain strictly $0
    let effective_cost = if provider.to_lowercase().contains("ollama")
        || provider.to_lowercase().contains("local")
    {
        0.0
    } else {
        cost_usd
    };

    let body = serde_json::json!({
        "id": generation_id,
        "traceId": trace_id,
        "name": format!("generation:{}", model),
        "startTime": start_time.to_rfc3339(),
        "endTime": end_time.to_rfc3339(),
        "model": model,
        "modelParameters": {
            "provider": provider,
        },
        "input": input_text,
        "output": output_text,
        "usage": {
            "promptTokens": prompt_tokens,
            "completionTokens": completion_tokens,
            "totalTokens": total_tokens,
        },
        "calculatedTotalCost": effective_cost,
        "metadata": {
            "provider": provider,
            "compute_time_ms": compute_time_ms,
        }
    });

    IngestionItem {
        id: Uuid::new_v4().to_string(),
        item_type: "generation-create".to_string(),
        timestamp: created_at.to_rfc3339(),
        body,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn create_span_item(
    span_id: &str,
    trace_id: &str,
    name: &str,
    start_time: &DateTime<Utc>,
    end_time: &DateTime<Utc>,
    level: &str,
    status_message: Option<&str>,
    metadata: serde_json::Value,
    input: Option<serde_json::Value>,
    output: Option<serde_json::Value>,
) -> IngestionItem {
    let mut body = serde_json::json!({
        "id": span_id,
        "traceId": trace_id,
        "name": name,
        "startTime": start_time.to_rfc3339(),
        "endTime": end_time.to_rfc3339(),
        "level": level,
        "metadata": metadata,
    });

    if let Some(msg) = status_message {
        body["statusMessage"] = serde_json::Value::String(msg.to_string());
    }
    if let Some(inp) = input {
        body["input"] = inp;
    }
    if let Some(out) = output {
        body["output"] = out;
    }

    IngestionItem {
        id: Uuid::new_v4().to_string(),
        item_type: "span-create".to_string(),
        timestamp: end_time.to_rfc3339(),
        body,
    }
}
