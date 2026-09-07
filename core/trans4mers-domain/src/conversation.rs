use crate::config::ModelConfig;
use crate::ids::{ConversationId, ProjectId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: ConversationId,
    pub project_id: ProjectId,
    pub title: String,
    pub status: ConversationStatus,
    pub settings: ConversationSettings,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
pub enum ConversationStatus {
    Active,
    Archived,
    Deleted,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConversationSettings {
    pub max_concurrent_agents: Option<u32>,
    pub model_config_override: Option<ModelConfig>,
}
