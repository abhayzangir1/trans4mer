use std::sync::Arc;
use tiktoken_rs::cl100k_base;
use tracing::{info, warn};
use trans4mers_domain::config::{CompactionConfig, ModelConfig};
use trans4mers_domain::execution::{ExecutionState, ReActStep};
use trans4mers_domain::provider::{LlmMessage, LlmProvider, LlmRequest};

pub struct ContextCompactor;

impl ContextCompactor {
    /// Checks if the execution history is too large and summarizes older steps
    /// to preserve the LLM's context window, respecting the user's CompactionConfig toggle.
    pub async fn compact_if_needed(
        state: &mut ExecutionState,
        _legacy_max_steps: usize,
        provider: Arc<dyn LlmProvider>,
        model_config: &ModelConfig,
        compaction_config: &CompactionConfig,
    ) {
        if !compaction_config.enabled {
            return;
        }

        let bpe = match cl100k_base() {
            Ok(b) => b,
            Err(e) => {
                warn!("Failed to initialize cl100k tokenizer: {}. Skipping compaction.", e);
                return;
            }
        };

        let max_context_tokens = model_config.context_limit.unwrap_or(8192) as usize;
        let safe_threshold =
            ((max_context_tokens as f32) * compaction_config.trigger_threshold_pct) as usize;

        let mut total_tokens = 0;
        let mut steps_tokens = Vec::with_capacity(state.steps.len());

        for step in &state.steps {
            let step_json = serde_json::to_string(step).unwrap_or_default();
            let tokens = bpe.encode_with_special_tokens(&step_json).len();
            total_tokens += tokens;
            steps_tokens.push(tokens);
        }

        if total_tokens > safe_threshold {
            info!(
                "Compacting context: {} tokens > {} threshold",
                total_tokens, safe_threshold
            );

            let mut current_tokens = total_tokens;
            let mut drop_count = 0;
            let preserve_recent = compaction_config.preserve_recent_messages.max(1);

            for token_count in steps_tokens {
                if state.steps.len().saturating_sub(drop_count) <= preserve_recent
                    || current_tokens <= safe_threshold / 2
                {
                    break;
                }
                current_tokens -= token_count;
                drop_count += 1;
            }

            if drop_count == 0 {
                return;
            }

            let old_steps: Vec<_> = state.steps.drain(0..drop_count).collect();

            let summary = if compaction_config.use_llm_summary {
                let old_steps_json = serde_json::to_string(&old_steps).unwrap_or_default();
                let request = LlmRequest {
                    messages: vec![
                        LlmMessage {
                            role: "system".to_string(),
                            content: "You are an internal summarization agent. Summarize the following execution steps into a single dense paragraph detailing the actions taken and their outcomes.".to_string(),
                        },
                        LlmMessage {
                            role: "user".to_string(),
                            content: old_steps_json,
                        }
                    ],
                    config: model_config.clone(),
                    tools: None,
                    response_schema: None,
                };

                match provider.generate(&request).await {
                    Ok(resp) => resp.content,
                    Err(e) => format!(
                        "Truncated {} steps (summarization failed: {})",
                        drop_count, e
                    ),
                }
            } else {
                format!(
                    "Pruned {} older historical execution steps to stay within context window.",
                    drop_count
                )
            };

            let summary_step = ReActStep {
                step_index: state.total_steps_executed,
                thought: format!(
                    "CONTEXT COMPACTION EVENT (summarized {} historical steps): {}",
                    drop_count, summary
                ),
                action_intent: None,
                result_payload: None,
                error: None,
                token_usage: None,
                created_at: chrono::Utc::now(),
                completed_at: Some(chrono::Utc::now()),
            };

            state.steps.insert(0, summary_step);
        }
    }
}
