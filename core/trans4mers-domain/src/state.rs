use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display, EnumString)]
pub enum AgentStatus {
    Active,  // Agent is available for execution
    Paused,  // Human paused the agent globally
    Retired, // Agent is archived/deleted
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display, EnumString)]
pub enum ExecutionStatus {
    Queued,             // Waiting for scheduler permit
    Running,            // Actively executing ReAct loop (holds permit)
    WaitingForTool,     // Yielded permit, async tool in progress
    WaitingForMessage,  // Yielded permit, waiting for delegation reply
    WaitingForApproval, // Yielded permit, waiting for human
    Checkpointing,      // Saving checkpoint (brief, holds permit)
    Completed,          // Agent declared task done
    Failed,             // Recoverable failure (RecoveryManager will retry)
    Cancelled,          // Human or CancellationToken cancelled
    TimedOut,           // Exceeded max_execution_duration_secs
}
