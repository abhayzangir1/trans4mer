use crate::ids::{AgentInstanceId, ConversationId, ExecutionId, TokenUsageId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// TokenUsage — persisted by the engine layer after each LLM call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub id: TokenUsageId,
    pub execution_id: ExecutionId,
    pub agent_instance_id: AgentInstanceId,
    pub conversation_id: ConversationId,
    pub provider: String,
    pub model: String,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    pub estimated_cost_usd: Option<f64>, // None for local providers
    pub compute_time_ms: u64,            // Meaningful for local models
    pub created_at: DateTime<Utc>,
}

/// TokenMetrics — returned by providers in LlmResponse (NOT persisted by providers).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenMetrics {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostEntry {
    pub id: String,
    pub provider: String,
    pub model: String,
    pub project_id: Option<String>,
    pub execution_id: Option<String>,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cost_usd: f32,
    pub is_fallback: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostBudget {
    pub id: String,
    pub project_id: Option<String>,
    pub provider: String,
    pub monthly_ceiling_usd: Option<f32>,
    pub daily_ceiling_usd: Option<f32>,
    pub hard_block: bool,
    pub alert_thresholds: Vec<f32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
