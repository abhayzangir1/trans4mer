use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocChunk {
    pub chunk_id: String,
    pub project_id: String,
    pub file_path: String,
    pub content_hash: String,
    pub ord: u32,
    pub line_start: usize,
    pub line_end: usize,
    pub text: String,
    pub kind: String,
    pub token_estimate: usize,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub embedding: Option<Vec<f32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocIngestState {
    pub project_id: String,
    pub file_path: String,
    pub content_hash: String,
    pub chunk_count: usize,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocSearchResult {
    pub chunk_id: String,
    pub file_path: String,
    pub line_start: usize,
    pub line_end: usize,
    pub snippet: String,
    pub kind: String,
    pub score: f32,
}
