use crate::ids::{AgentInstanceId, ExecutionId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// EffectClass — the mathematical safety classification of a tool.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
pub enum EffectClass {
    ReadOnly,
    IdempotentMutation { requires_idempotency_key: bool },
    NonIdempotentMutation,
    Unknown,
}

/// RiskLevel — the danger level if a tool is misused.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
pub enum RiskLevel {
    Safe,     // No side effects (read operations)
    Low,      // Reversible side effects (write to workspace file)
    Medium,   // Significant side effects (run command, git commit)
    High,     // Dangerous (git push, install software, network request)
    Critical, // Potentially destructive (delete files, run as root)
}

/// ToolManifest — declares a tool's interface and safety properties.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolManifest {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value, // JSON Schema
    pub output_schema: Option<serde_json::Value>,
    pub effect_class: EffectClass,
    pub baseline_risk: RiskLevel,
    pub required_capabilities: Vec<Capability>,
    pub source: ToolSource,
    pub verification_command: Option<String>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
pub enum ToolSource {
    Native, // Built-in Rust tool in trans4mers-engine
    Mcp,    // External MCP server
    Plugin, // Plugin-provided tool
}

/// ToolRequest — an agent's request to execute a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRequest {
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub requesting_agent_id: AgentInstanceId,
    pub project_id: crate::ids::ProjectId,
    pub execution_id: ExecutionId,
    pub workspace_root: std::path::PathBuf,
    pub idempotency_key: Option<String>,
}

/// ToolResult — the outcome of a tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub content: String,
    pub error: Option<String>,
    pub artifacts: Vec<String>, // File paths produced
    pub metadata: HashMap<String, serde_json::Value>,
}

/// ToolCallRecord — persisted record of a tool invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRecord {
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub effect_class: EffectClass,
    pub idempotency_key: Option<String>,
}

/// Capability — a permission that an agent may hold.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
pub enum Capability {
    FilesystemRead,
    FilesystemWrite,
    ShellExecute,
    GitRead,
    GitWrite,
    GitPush,
    BrowserNavigate,
    BrowserInteract,
    NetworkRequest,
    MemoryRead,
    MemoryWrite,
    AgentSpawn,
    AgentMessage,
    SecretRead,
    MemorizeRule,
    Custom(String),
}
use crate::error::Trans4mersError;
use async_trait::async_trait;

/// The foundational async interface for all extensible capabilities.
#[async_trait]
pub trait Tool: Send + Sync {
    /// The canonical manifest declaring this tool's capabilities.
    fn manifest(&self) -> &ToolManifest;

    /// Executes the tool payload asynchronously.
    async fn execute(&self, request: &ToolRequest) -> Result<ToolResult, Trans4mersError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerEntry {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub auto_launch: bool,
    pub approved: bool,
    pub notes: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}
