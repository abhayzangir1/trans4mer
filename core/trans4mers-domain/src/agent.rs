use crate::config::ModelConfig;
use crate::ids::*;
use crate::state::AgentStatus;
use crate::tool::Capability;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// AgentDefinition — a reusable template for creating agent instances.
/// Stored in the GLOBAL database (global.db).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinition {
    pub id: AgentDefinitionId,
    pub name: String,
    pub role: String,
    pub description: String,
    pub system_instructions: String,
    pub default_model_config: ModelConfig,
    pub baseline_capabilities: Vec<Capability>,
    pub default_skills: Vec<String>,
    pub default_tools: Vec<String>,
    pub metadata: HashMap<String, String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// AgentInstance — a running/persistent agent identity within a project.
/// Stored in the PROJECT database (project.db).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInstance {
    pub id: AgentInstanceId,
    pub project_id: ProjectId,
    pub definition_id: AgentDefinitionId,
    pub parent_instance_id: Option<AgentInstanceId>,
    pub status: AgentStatus,
    pub capabilities: Vec<Capability>,
    pub model_config_override: Option<ModelConfig>,
    pub depth_level: u32, // 0 = top-level, 1 = sub-agent, 2 = sub-sub-agent...
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// AgentMembership — many-to-many relationship between agents and conversations.
/// Stored in the PROJECT database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMembership {
    pub agent_instance_id: AgentInstanceId,
    pub conversation_id: ConversationId,
    pub role_in_conversation: String, // e.g., "Frontend Coder", "Reviewer"
    pub joined_at: DateTime<Utc>,
    pub left_at: Option<DateTime<Utc>>, // NULL = currently active
}
