use serde::{Deserialize, Serialize};

/// Classification of a failure during the agent lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureClass {
    /// F1: Data or State failure (e.g. database locking, missing file)
    State,

    /// F2: Logical failure (e.g. tool crashed, invalid arguments)
    Logic,

    /// F3: Hallucination or parsing failure (e.g. LLM returned malformed JSON)
    Hallucination,

    /// F4: Infrastructure or API failure (e.g. LLM provider timeout, 502 Bad Gateway)
    Api,
}

/// The deterministic policy dictated by the self-healing engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryPolicy {
    pub can_retry: bool,
    pub max_retries: u32,
    pub backoff_ms: u64,
    pub requires_context_compaction: bool,
    pub fallback_to_delegation: bool,
}
