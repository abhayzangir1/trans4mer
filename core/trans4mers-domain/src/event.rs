use crate::ids::*;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum DomainEvent {
    ProjectCreated {
        project_id: ProjectId,
        name: String,
        workspace_path: String,
    },
    ConversationCreated {
        conversation_id: ConversationId,
        project_id: ProjectId,
        title: String,
    },
    AgentInstanceCreated {
        agent_instance_id: AgentInstanceId,
        definition_id: AgentDefinitionId,
        project_id: ProjectId,
    },
    AgentStatusChanged {
        agent_instance_id: AgentInstanceId,
        old_status: crate::state::AgentStatus,
        new_status: crate::state::AgentStatus,
    },

    // Execution
    ExecutionStarted {
        execution_id: ExecutionId,
        agent_instance_id: AgentInstanceId,
    },
    ExecutionStepCompleted {
        execution_id: ExecutionId,
        step_number: u32,
    },
    ExecutionStatusChanged {
        execution_id: ExecutionId,
        old_status: crate::state::ExecutionStatus,
        new_status: crate::state::ExecutionStatus,
    },
    ExecutionCompleted {
        execution_id: ExecutionId,
    },
    ExecutionFailed {
        execution_id: ExecutionId,
        reason: String,
    },
    CheckpointSaved {
        execution_id: ExecutionId,
        step_number: u32,
        phase: String,
    },

    // Streaming & Messaging
    TextDelta {
        execution_id: ExecutionId,
        delta: String,
    },
    MessageSent {
        message: crate::message::Message,
    },
    MessageCreated {
        message: crate::message::Message,
    },
    MessageQueued {
        message_id: String,
        agent_id: AgentInstanceId,
        payload: String,
    },
    InboxMessageQueued {
        message_id: String,
        recipient_agent_id: AgentInstanceId,
        sender_actor_id: String,
        payload: String,
    },
    InboxMessageClaimed {
        message_id: String,
        agent_id: AgentInstanceId,
    },
    InboxMessageAcked {
        message_id: String,
        agent_id: AgentInstanceId,
    },

    // UI & System
    AgentSpawned {
        agent_id: AgentInstanceId,
        project_id: ProjectId,
        definition_id: String,
        parent_id: Option<String>,
    },
    ToolExecuted {
        execution_id: ExecutionId,
        tool_name: String,
        success: bool,
    },
    PolicyEvaluated {
        execution_id: ExecutionId,
        decision: String,
    },

    // Approvals
    PendingApproval {
        approval_id: String,
        execution_id: ExecutionId,
        conversation_id: crate::ids::ConversationId,
        agent_id: AgentInstanceId,
        capability: String,
        tool_name: String,
        payload: String,
        risk_level: String,
        arguments_hash: Option<String>,
    },
    ApprovalResolved {
        approval_id: String,
        approved: bool,
        feedback: Option<String>,
    },
    ApprovalExpired {
        approval_id: ApprovalId,
    },

    // Memory
    MemoryStored {
        project_id: ProjectId,
        memory_id: String,
    },
    RuleLearned {
        project_id: ProjectId,
        rule_id: String,
    },
    ArtifactCreated {
        project_id: ProjectId,
        artifact_id: String,
    },

    // Terminal
    TerminalOutput {
        terminal_id: String,
        data: String,
    },

    // Workflows
    WorkflowRunStarted {
        run_id: WorkflowRunId,
        workflow_id: WorkflowId,
    },
    WorkflowNodeStarted {
        run_id: WorkflowRunId,
        node_id: WorkflowNodeId,
    },
    WorkflowNodeCompleted {
        run_id: WorkflowRunId,
        node_id: WorkflowNodeId,
    },
    WorkflowRunCompleted {
        run_id: WorkflowRunId,
    },
    WorkflowRunFailed {
        run_id: WorkflowRunId,
        reason: String,
    },

    // Memory Pyramid & Trust
    MemoryTierPromoted {
        project_id: ProjectId,
        memory_id: String,
        old_tier: String,
        new_tier: String,
    },
    DiffReviewRequested {
        project_id: ProjectId,
        diff_id: String,
        capability: String,
        risk_level: String,
    },
    DiffReviewResolved {
        project_id: ProjectId,
        diff_id: String,
        decision: String,
    },
    SkillProposed {
        project_id: ProjectId,
        skill_name: String,
        diff_id: String,
    },
    CostAlertFired {
        project_id: Option<ProjectId>,
        provider: String,
        current_usd: f32,
        ceiling_usd: f32,
        message: String,
    },

    // Interactive Human Sync & Integrations
    FileModifiedByHuman {
        project_id: ProjectId,
        relative_path: String,
        diff_summary: String,
    },
    ArtifactCommentAdded {
        project_id: ProjectId,
        artifact_id: String,
        comment_id: String,
        author: String,
        content: String,
    },
    TelegramMessageReceived {
        chat_id: i64,
        text: String,
        project_id: String,
    },

    // Protocol Traffic & External Integrations
    McpFrameLogged {
        frame: McpTrafficFrame,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum McpTrafficDirection {
    #[serde(rename = "inbound")]
    Inbound,
    #[serde(rename = "outbound")]
    Outbound,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTrafficFrame {
    pub id: String,
    pub server_name: String,
    pub direction: McpTrafficDirection,
    pub method: Option<String>,
    pub payload: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub sequence_id: i64,
    pub event_id: EventId,
    pub event: DomainEvent,
    pub actor_id: ActorId,
    pub signature: Option<String>,
    pub created_at: DateTime<Utc>,
}
