use crate::ids::ProjectId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: ProjectId,
    pub name: String,
    pub workspace_path: String, // Absolute path to user's project folder
    pub description: Option<String>,
    pub global_instructions: Option<String>,
    pub settings: ProjectSettings,
    pub embedding_dimensions: u32, // Locked at project creation for sqlite-vec
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn default_vector_backend() -> String {
    "sqlite-vec".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSettings {
    pub default_provider: Option<String>,
    pub default_model: Option<String>,
    pub max_concurrent_agents: u32,       // Default: 8
    pub max_agent_depth: u32,             // Default: 5
    pub max_execution_steps: u32,         // Default: 30
    pub max_execution_duration_secs: u64, // Default: 3600 (1 hour)
    #[serde(default = "default_vector_backend")]
    pub vector_backend: String, // Default: "sqlite-vec", or "lancedb"
}

impl Default for ProjectSettings {
    fn default() -> Self {
        Self {
            default_provider: Some("ollama".to_string()),
            default_model: Some("llama3.1:8b".to_string()),
            max_concurrent_agents: 8,
            max_agent_depth: 5,
            max_execution_steps: 30,
            max_execution_duration_secs: 3600,
            vector_backend: default_vector_backend(),
        }
    }
}
