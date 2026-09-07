use crate::ids::{AgentInstanceId, ApprovalId, ExecutionId, ProjectId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionDiff {
    pub id: String,
    pub project_id: ProjectId,
    pub execution_id: Option<ExecutionId>,
    pub agent_instance_id: Option<AgentInstanceId>,
    pub capability: String,
    pub risk_level: String,
    pub kind: DiffKind,
    pub diff_payload: String,
    pub hunks: Vec<DiffHunk>,
    pub decision: DiffDecision,
    pub force_review_reason: Option<String>,
    pub approval_id: Option<ApprovalId>,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "details")]
pub enum DiffKind {
    FilePatch {
        file_path: String,
        additions: usize,
        deletions: usize,
        is_new: bool,
        is_deleted: bool,
    },
    CommandExec {
        command: String,
        args: Vec<String>,
        cwd: Option<String>,
    },
    MemoryMutation {
        old_content: Option<String>,
        new_content: String,
        tier: String,
        importance: f32,
    },
    BrowserAction {
        verb: String,
        url: String,
        selector: Option<String>,
    },
    SkillProposal {
        skill_name: String,
        description: String,
        trigger_pattern: String,
        action_template: String,
    },
    ConfigChange {
        key: String,
        old_val: Option<String>,
        new_val: String,
    },
    GitHubPrReview {
        owner: String,
        repo: String,
        pull_number: u64,
        review_event: String,
        summary: String,
        comments_count: usize,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffHunk {
    #[serde(alias = "id")]
    pub hunk_index: usize,
    pub old_start: usize,
    pub old_lines: usize,
    pub new_start: usize,
    pub new_lines: usize,
    pub header: String,
    pub lines: Vec<String>,
    #[serde(alias = "approved")]
    pub accepted: bool,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
pub enum DiffDecision {
    Pending,
    AutoApproved,
    Approved,
    Rejected,
    PartiallyApproved,
}
