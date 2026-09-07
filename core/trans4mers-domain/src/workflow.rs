use crate::ids::*;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub id: WorkflowId,
    pub conversation_id: ConversationId,
    pub name: String,
    pub description: String,
    pub nodes: Vec<WorkflowNode>,
    pub edges: Vec<WorkflowEdge>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowNode {
    pub id: WorkflowNodeId,
    pub workflow_id: WorkflowId,
    pub name: String,
    pub node_type: WorkflowNodeType,
    pub agent_definition_id: Option<AgentDefinitionId>,
    pub task_description: Option<String>,
    pub position_x: f64,
    pub position_y: f64,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
pub enum WorkflowNodeType {
    Start,
    AgentTask,
    HumanReview,
    Condition,
    Parallel,
    End,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowEdge {
    pub id: uuid::Uuid,
    pub workflow_id: WorkflowId,
    pub source_node_id: WorkflowNodeId,
    pub target_node_id: WorkflowNodeId,
    pub condition: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRun {
    pub id: WorkflowRunId,
    pub workflow_id: WorkflowId,
    pub status: WorkflowRunStatus,
    pub current_node_id: Option<WorkflowNodeId>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
pub enum WorkflowRunStatus {
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}
