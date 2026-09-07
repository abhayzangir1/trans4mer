use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::recovery::{FailureClass, RecoveryPolicy};
use trans4mers_domain::tool::EffectClass;

pub struct SelfHealing;

impl SelfHealing {
    /// Classifies an error and determines the mathematical recovery policy.
    pub fn handle_error(error: &Trans4mersError, tool_effect: EffectClass) -> RecoveryPolicy {
        let class = Self::classify(error);

        match class {
            FailureClass::Api => RecoveryPolicy {
                can_retry: true,
                max_retries: 5,
                backoff_ms: 2000,
                requires_context_compaction: false,
                fallback_to_delegation: false,
            },
            FailureClass::Hallucination => RecoveryPolicy {
                can_retry: true,
                max_retries: 3,
                backoff_ms: 0,
                requires_context_compaction: true,
                fallback_to_delegation: true,
            },
            FailureClass::State => RecoveryPolicy {
                can_retry: true,
                max_retries: 3,
                backoff_ms: 500, // Wait for locks to clear
                requires_context_compaction: false,
                fallback_to_delegation: true,
            },
            FailureClass::Logic => {
                // If it's a mutation without idempotency, we CANNOT safely retry automatically.
                let can_retry = match tool_effect {
                    EffectClass::ReadOnly => true,
                    EffectClass::IdempotentMutation { .. } => true,
                    EffectClass::NonIdempotentMutation => false,
                    EffectClass::Unknown => false,
                };

                RecoveryPolicy {
                    can_retry,
                    max_retries: if can_retry { 2 } else { 0 },
                    backoff_ms: 0,
                    requires_context_compaction: false,
                    fallback_to_delegation: true, // Ask human/parent for help
                }
            }
        }
    }

    fn classify(error: &Trans4mersError) -> FailureClass {
        match error {
            Trans4mersError::LlmProvider { .. }
            | Trans4mersError::LlmTimeout { .. }
            | Trans4mersError::BrowserTimeout { .. } => FailureClass::Api,
            Trans4mersError::Internal(msg) if msg.contains("JSON") || msg.contains("parse") => {
                FailureClass::Hallucination
            }
            Trans4mersError::ToolExecution { .. } => FailureClass::Logic,
            Trans4mersError::Database(_) => FailureClass::State,
            _ => FailureClass::Logic,
        }
    }
}
