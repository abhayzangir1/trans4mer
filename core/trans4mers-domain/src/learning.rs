use crate::ids::{LearnedRuleId, ProjectId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnedRule {
    pub id: LearnedRuleId,
    pub project_id: Option<ProjectId>, // NULL = Global rule
    pub rule_text: String,
    pub confidence: Confidence,
    pub metadata: RuleMetadata,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub embedding: Option<Vec<f32>>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
pub enum Confidence {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleMetadata {
    pub language: Option<String>,     // e.g., "rust", "typescript"
    pub framework: Option<String>,    // e.g., "react", "tauri"
    pub file_pattern: Option<String>, // e.g., "*.tsx", "src/store/*"
    pub source_agent_id: Option<String>,
    pub source_execution_id: Option<String>,
}
