use crate::ids::{AgentInstanceId, ConversationId, TaskId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub conversation_id: ConversationId,
    pub assigned_agent_id: Option<AgentInstanceId>,
    pub parent_task_id: Option<TaskId>,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    pub priority: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
pub enum TaskStatus {
    Pending,
    Assigned,
    InProgress,
    Blocked,
    Completed,
    Failed,
    Cancelled,
}
