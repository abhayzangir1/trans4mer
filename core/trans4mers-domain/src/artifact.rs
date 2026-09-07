use crate::ids::{ArtifactId, ExecutionId, ProjectId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub id: ArtifactId,
    pub project_id: ProjectId,
    pub producer_execution_id: Option<ExecutionId>,
    pub relative_path: String,
    pub content_hash: String, // SHA-256 hex digest
    pub mime_type: String,
    pub size_bytes: u64,
    pub created_at: DateTime<Utc>,
}
