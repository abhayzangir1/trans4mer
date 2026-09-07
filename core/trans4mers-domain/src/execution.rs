use crate::ids::*;
use crate::state::ExecutionStatus;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// AgentExecution — a specific run of an agent's reasoning loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentExecution {
    pub id: ExecutionId,
    pub agent_instance_id: AgentInstanceId,
    pub task_id: Option<TaskId>,
    pub conversation_id: ConversationId,
    pub status: ExecutionStatus,
    pub generation: u64, // Incremented on each resume — prevents stale events
    pub current_step: u32,
    pub max_steps: u32,
    pub trigger_message_id: Option<MessageId>,
    pub started_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// ExecutionStep — a single step in the ReAct loop, persisted for observability.
/// Uses SEMANTIC logging — no raw LLM strings stored here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionStep {
    pub id: uuid::Uuid,
    pub execution_id: ExecutionId,
    pub step_number: u32,
    pub step_type: StepType,
    pub action_intent: String, // What the agent intended to do
    pub tool_call_request: Option<serde_json::Value>, // Structured, not raw string
    pub tool_result_summary: String, // Summary, not raw output
    pub decision_rationale: String, // Why the agent made this decision
    pub token_usage: Option<StepTokenUsage>,
    pub duration_ms: u64,
    pub created_at: DateTime<Utc>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
pub enum StepType {
    Reasoning,         // LLM thinking
    ToolCall,          // Calling a tool
    ToolResult,        // Receiving tool result
    ContextCompaction, // Summarizing old context
    Checkpoint,        // Saving checkpoint
    Delegation,        // Spawning sub-agent
    MessageSend,       // Sending message to channel/DM
    Completion,        // Declaring task complete
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepTokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Checkpoint — persisted execution state for crash recovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub id: CheckpointId,
    pub execution_id: ExecutionId,
    pub generation: u64,
    pub step_number: u32,
    pub execution_status: ExecutionStatus,
    pub execution_phase: ExecutionPhase, // Logical phase for recovery
    pub inbox_cursor: Option<String>,    // Last ACKED inbox message ID
    pub pending_tool_state: Option<serde_json::Value>,
    pub last_event_sequence: i64, // Primary logical recovery pointer
    pub context_snapshot: Option<serde_json::Value>, // Optional performance optimization (avoid full replay)
    pub created_at: DateTime<Utc>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
pub enum ExecutionPhase {
    ContextAssembly,    // Building the LLM prompt
    LlmGeneration,      // Waiting for LLM response
    ToolExecution,      // Tool is running
    WaitingForApproval, // Waiting for human
    WaitingForMessage,  // Waiting for agent reply
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionState {
    pub execution_id: ExecutionId,
    pub conversation_id: ConversationId,
    pub steps: Vec<ReActStep>,
    #[serde(default)]
    pub total_steps_executed: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReActStep {
    pub step_index: u32,
    pub thought: String,
    pub action_intent: Option<serde_json::Value>,
    pub result_payload: Option<serde_json::Value>,
    pub error: Option<StepError>,
    pub token_usage: Option<crate::token_usage::TokenMetrics>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepError {
    pub error_type: String,
    pub message: String,
    pub stack_trace: Option<String>,
}
