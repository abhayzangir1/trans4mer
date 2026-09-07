use crate::ids::{AgentInstanceId, ConversationId, EventId, MemoryId, MessageId, ProjectId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: MemoryId,
    pub project_id: ProjectId,
    pub conversation_id: Option<ConversationId>,
    pub agent_instance_id: Option<AgentInstanceId>,
    pub scope: MemoryScope,
    pub lifecycle: MemoryLifecycle,
    pub content: String,
    pub embedding: Option<Vec<f32>>, // Vector embedding for semantic search
    pub importance: f32,             // 0.0 to 1.0
    pub confidence: f32,             // 0.0 to 1.0
    pub provenance: MemoryProvenance,
    pub depth_level: u32,     // Agent depth that created this
    pub tier: MemoryTier,     // 4-Tier pyramid level
    pub retrieval_count: u32, // Number of times returned by RAG
    pub last_retrieved_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>, // For EPHEMERAL scope only
    #[serde(default)]
    pub valid_from: Option<DateTime<Utc>>,
    #[serde(default)]
    pub valid_until: Option<DateTime<Utc>>,
    #[serde(default)]
    pub valid_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub invalid_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub replaced_by: Option<String>,
    #[serde(default)]
    pub content_hash: Option<String>,
    pub visibility_overrides: Option<serde_json::Value>, // JSON array of Actor ID strings for exceptions
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
pub enum MemoryTier {
    Working,    // Level 1: Immediate conversational context
    Episodic,   // Level 2: Project-level execution trace and outcomes
    Semantic,   // Level 3: Distilled facts, verified knowledge, and preferences
    Procedural, // Level 4: Global rules, heuristics, and terminal policies
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
pub enum MemoryScope {
    Global,       // Visible to all agents across all projects
    Project,      // Visible to all agents within a project
    Conversation, // Visible to agents in this conversation
    Agent,        // Private to the owning agent + visible to parent chain
    Ephemeral,    // Private to the owning agent, auto-expires
    Artifact,     // Visible to siblings + parent chain (produced outputs)
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
pub enum MemoryLifecycle {
    Observation, // Raw observation from agent activity
    Candidate,   // Proposed for persistence
    Validated,   // Validated (by human or critic)
    Persisted,   // Permanently stored
    Rejected,    // Rejected during validation
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryProvenance {
    pub source_event_id: Option<EventId>,
    pub source_message_id: Option<MessageId>,
    pub source_agent_id: Option<AgentInstanceId>,
    pub creation_reason: String,
}
