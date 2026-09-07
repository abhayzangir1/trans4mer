use crate::tool::EffectClass;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Trans4mersError {
    // Storage errors
    #[error("Database error: {0}")]
    Database(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Migration error: {0}")]
    Migration(String),

    // Agent errors
    #[error("Invalid state transition: {from} -> {to}")]
    InvalidStateTransition { from: String, to: String },

    #[error("Agent not found: {0}")]
    AgentNotFound(String),

    #[error("Execution limit exceeded: step {step} of {max}")]
    ExecutionLimitExceeded { step: u32, max: u32 },

    // LLM errors
    #[error("LLM provider error: {provider} - {message}")]
    LlmProvider { provider: String, message: String },

    #[error("LLM timeout after {timeout_secs}s")]
    LlmTimeout { timeout_secs: u64 },

    // Tool errors
    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    #[error("Tool execution failed: {tool} - {message}")]
    ToolExecution { tool: String, message: String },

    #[error("Tool verification failed: {0}")]
    VerificationFailed(String),

    // Policy errors
    #[error("Action denied by policy: {capability}")]
    PolicyDenied { capability: String },

    #[error("Approval required for: {action}")]
    ApprovalRequired { action: String, approval_id: String },

    #[error("Approval rejected: {0}")]
    ApprovalRejected(String),

    // Memory errors
    #[error("Memory access denied: agent {agent_id} cannot access scope {scope}")]
    MemoryAccessDenied { agent_id: String, scope: String },

    // Filesystem errors
    #[error("Path traversal detected: {0}")]
    PathTraversal(String),

    #[error("Path outside workspace: {0}")]
    PathOutsideWorkspace(String),

    // Recovery errors
    #[error("Stale generation: expected {expected}, got {actual}")]
    StaleGeneration { expected: u64, actual: u64 },

    #[error("Checkpoint not found: {0}")]
    CheckpointNotFound(String),

    // MCP/Plugin errors
    #[error("MCP server error: {server} - {message}")]
    McpServer { server: String, message: String },

    #[error("Plugin error: {plugin} - {message}")]
    Plugin { plugin: String, message: String },

    #[error("Plugin timeout after {timeout_secs}s")]
    PluginTimeout { plugin: String, timeout_secs: u64 },

    // Terminal errors
    #[error("Terminal error: {0}")]
    Terminal(String),

    // Config errors
    #[error("Configuration error: {0}")]
    Config(String),

    // Browser errors
    #[error("Browser automation error: {0}")]
    Browser(String),

    #[error("Browser executable not found: {0}")]
    BrowserNotInstalled(String),

    #[error("Browser element not interactable: {selector} - {reason}")]
    BrowserElementNotInteractable { selector: String, reason: String },

    #[error("Browser timeout waiting for {condition} after {timeout_ms}ms")]
    BrowserTimeout { condition: String, timeout_ms: u64 },

    // Generic
    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Filesystem error: {0}")]
    Filesystem(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Cancelled")]
    Cancelled,
}

/// RecoveryPolicy — attached to errors to guide recovery behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecoveryPolicy {
    Retry {
        max_attempts: u32,
        backoff_base_ms: u64,
    },
    Fallback {
        fallback_provider: String,
        fallback_model: String,
    },
    EscalateToHuman {
        message: String,
    },
    Fail {
        reason: String,
    },
}

impl Trans4mersError {
    /// Returns the recommended recovery policy for this error.
    pub fn recovery_policy(&self, effect_class: EffectClass) -> RecoveryPolicy {
        match effect_class {
            EffectClass::NonIdempotentMutation | EffectClass::Unknown => {
                RecoveryPolicy::EscalateToHuman {
                    message: format!("Non-idempotent operation failed: {}", self),
                }
            }
            EffectClass::ReadOnly | EffectClass::IdempotentMutation { .. } => match self {
                Self::LlmProvider { .. } => RecoveryPolicy::Retry {
                    max_attempts: 3,
                    backoff_base_ms: 1000,
                },
                Self::LlmTimeout { .. } => RecoveryPolicy::Retry {
                    max_attempts: 2,
                    backoff_base_ms: 2000,
                },
                Self::ToolExecution { .. } => RecoveryPolicy::Retry {
                    max_attempts: 2,
                    backoff_base_ms: 500,
                },
                Self::McpServer { .. } | Self::Plugin { .. } => RecoveryPolicy::Retry {
                    max_attempts: 3,
                    backoff_base_ms: 1000,
                },
                Self::Network(_) => RecoveryPolicy::Retry {
                    max_attempts: 3,
                    backoff_base_ms: 1000,
                },
                Self::PolicyDenied { .. } | Self::ApprovalRejected(_) => RecoveryPolicy::Fail {
                    reason: "Action denied by policy or human".to_string(),
                },
                Self::ApprovalRequired { .. } => RecoveryPolicy::EscalateToHuman {
                    message: "Human approval required".to_string(),
                },
                Self::VerificationFailed(_) => RecoveryPolicy::Retry {
                    max_attempts: 3,
                    backoff_base_ms: 500,
                },
                Self::BrowserTimeout { .. } => RecoveryPolicy::Retry {
                    max_attempts: 2,
                    backoff_base_ms: 1000,
                },
                Self::BrowserElementNotInteractable { .. } => RecoveryPolicy::Retry {
                    max_attempts: 2,
                    backoff_base_ms: 500,
                },
                _ => RecoveryPolicy::Fail {
                    reason: format!("{}", self),
                },
            },
        }
    }
}

impl From<rusqlite::Error> for Trans4mersError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Database(e.to_string())
    }
}

impl From<Trans4mersError> for String {
    fn from(e: Trans4mersError) -> Self {
        e.to_string()
    }
}
