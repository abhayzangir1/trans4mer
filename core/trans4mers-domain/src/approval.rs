use crate::ids::{AgentInstanceId, ApprovalId, ConversationId, ExecutionId};
use crate::tool::{Capability, RiskLevel};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Approval {
    pub id: ApprovalId,
    pub execution_id: ExecutionId,
    pub agent_instance_id: AgentInstanceId,
    pub conversation_id: ConversationId,
    pub capability: Capability,
    pub tool_name: Option<String>,
    pub action_description: String,
    pub arguments_summary: String,
    pub arguments_hash: Option<String>,
    pub risk_level: RiskLevel,
    pub status: ApprovalStatus,
    pub human_feedback: Option<String>,
    pub requested_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>, // Always set — 24h default
    pub resolved_at: Option<DateTime<Utc>>,
}

impl Approval {
    pub fn default_expiry() -> DateTime<Utc> {
        Utc::now() + Duration::hours(24)
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Rejected,
    Expired,
    Cancelled,
}
