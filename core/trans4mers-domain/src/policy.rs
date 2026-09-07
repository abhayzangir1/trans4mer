use crate::ids::{AgentInstanceId, ProjectId};
use crate::tool::Capability;
use serde::{Deserialize, Serialize};

/// Policy — a rule that governs a capability at a specific scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    pub capability: Capability,
    pub outcome: PolicyOutcome,
    pub scope: PolicyScope,
    pub conditions: Option<PolicyConditions>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
pub enum PolicyOutcome {
    Allow, // Proceed without human approval
    Ask,   // Require human approval before proceeding
    Deny,  // Block this action entirely
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PolicyScope {
    Global,
    Project(ProjectId),
    Agent(AgentInstanceId),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyConditions {
    pub allowed_paths: Option<Vec<String>>, // For filesystem operations
    pub blocked_paths: Option<Vec<String>>,
    pub allowed_commands: Option<Vec<String>>, // For shell operations
    pub blocked_commands: Option<Vec<String>>,
    pub max_retries: Option<u32>,
}
