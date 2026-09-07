use crate::actor::Actor;
use crate::ids::{AgentInstanceId, ApprovalId, ChannelId, ConversationId, MessageId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: MessageId,
    pub conversation_id: ConversationId,
    pub channel_id: ChannelId,
    pub thread_id: Option<MessageId>, // Reply thread — references parent message
    pub sender: Actor,                // Unified Actor type
    pub content: String,
    pub message_kind: MessageKind,
    pub mentions: Vec<Mention>,
    pub attachments: Vec<Attachment>,
    pub requires_approval: bool,
    pub approval_id: Option<ApprovalId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
pub enum MessageKind {
    Chat,               // Normal message
    ToolCall,           // Agent requesting a tool
    ToolResult,         // Result from a tool
    ApprovalRequest,    // Requesting human approval
    ApprovalResponse,   // Human's approval/rejection
    SystemNotification, // System event notification
    DelegationRequest,  // Agent requesting to spawn sub-agent
    StatusUpdate,       // Agent execution status change
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mention {
    pub agent_instance_id: AgentInstanceId,
    pub display_name: String,
    pub start_offset: usize,
    pub end_offset: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub filename: String,
    pub file_path: String,
    pub mime_type: String,
    pub size_bytes: u64,
}
