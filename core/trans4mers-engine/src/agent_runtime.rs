use crate::app_state::AppState;
use crate::context_compactor::ContextCompactor;
use crate::context_engine::ContextEngine;
use crate::memory_engine::MemoryEngine;
use crate::policy_engine::PolicyEngine;
use crate::self_healing::SelfHealing;
use std::sync::Arc;
use tokio::sync::OwnedSemaphorePermit;
use tokio_util::sync::CancellationToken;
use trans4mers_domain::config::ModelConfig;
use trans4mers_domain::diff::{DiffDecision, DiffKind};
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::execution::{ExecutionState, ReActStep};
use trans4mers_domain::ids::{AgentInstanceId, ProjectId};
use trans4mers_domain::provider::{LlmProvider, LlmRequest};
use trans4mers_domain::tool::EffectClass;

static BUDGET_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The core ReAct loop for an agent execution.
/// It strictly yields its semaphore permit when not actively computing.
#[allow(clippy::too_many_arguments)]
pub async fn run_agent_execution(
    mut state: ExecutionState,
    provider: Arc<dyn LlmProvider>,
    model_config: ModelConfig,
    _permit: OwnedSemaphorePermit, // Proves we have a slot to run. Dropped when yielding.
    global_instructions: &str,
    memory_engine: Option<&MemoryEngine>, // Injected dependency
    agent_id: &AgentInstanceId,
    project_id: &ProjectId,
    workspace_path_override: Option<std::path::PathBuf>,
    app_state: Arc<AppState>,
    cancellation_token: CancellationToken,
) -> Result<ExecutionState, Trans4mersError> {
    let mut current_permit = Some(_permit);
    let mut target_channel_id = trans4mers_domain::ids::ChannelId::from_str("general")
        .unwrap_or_else(|_| trans4mers_domain::ids::ChannelId::new());
    let mut target_convo_id = state.conversation_id;
    let mut current_sequence_id: i64;

    // CQRS: Emit ExecutionStarted
    {
        let event = trans4mers_domain::event::DomainEvent::ExecutionStarted {
            execution_id: state.execution_id,
            agent_instance_id: *agent_id,
        };
        let db = app_state
            .get_project_db(project_id)
            .ok_or_else(|| Trans4mersError::Database("Project DB not found".to_string()))?;
        let envelope = db
            .with_write_tx(|conn| {
                crate::cqrs::commit_event(
                    conn,
                    event,
                    trans4mers_domain::ids::ActorId::from_uuid(*agent_id.as_uuid()),
                )
            })
            .map_err(|e| Trans4mersError::Database(e.to_string()))?;
        current_sequence_id = envelope.sequence_id;
        let env_arc = std::sync::Arc::new(envelope);
        let _ = app_state.get_event_bus(project_id).publish(env_arc.clone());
        let _ = app_state.global_event_bus.publish(env_arc);
    }

    // Core ReAct Loop
    loop {
        // 0. Check for hard cancellation via the Scheduler
        if cancellation_token.is_cancelled() {
            return Err(Trans4mersError::Internal(
                "Execution cancelled by system".to_string(),
            ));
        }

        // 0.5. Check Inbox (Section 8.1 - ReAct Inbox)
        let db = app_state
            .get_project_db(project_id)
            .ok_or_else(|| Trans4mersError::Database("Project DB not found".to_string()))?;
        let claimed_msg_id_opt = if let Some((msg_id, payload)) =
            db.with_write_tx(|conn| crate::agent_inbox::AgentInbox::claim_next(conn, agent_id))?
        {
            let (prompt_text, payload_value) = if let Ok(incoming_msg) =
                serde_json::from_value::<trans4mers_domain::message::Message>(payload.clone())
            {
                target_channel_id = incoming_msg.channel_id.clone();
                target_convo_id = incoming_msg.conversation_id;
                (
                    format!(
                        "User Instruction from {}: {}",
                        incoming_msg.sender.display_name(),
                        incoming_msg.content
                    ),
                    serde_json::to_value(&incoming_msg).unwrap_or(serde_json::Value::Null),
                )
            } else {
                (
                    format!("I received a new message from my inbox: {:?}", payload),
                    serde_json::to_value(&payload).unwrap_or(serde_json::Value::Null),
                )
            };

            state.total_steps_executed += 1;
            state.steps.push(ReActStep {
                step_index: state.total_steps_executed,
                thought: prompt_text,
                action_intent: None,
                result_payload: Some(payload_value),
                error: None,
                token_usage: None,
                created_at: chrono::Utc::now(),
                completed_at: Some(chrono::Utc::now()),
            });
            Some(msg_id)
        } else {
            None
        };

        // 0.6 Checkpoint (Section 18.1 - Crash Recovery)
        let context_snapshot = serde_json::to_value(&state).ok();
        let cp = trans4mers_domain::execution::Checkpoint {
            id: trans4mers_domain::ids::CheckpointId::new(),
            execution_id: state.execution_id,
            generation: state.total_steps_executed as u64,
            step_number: state.total_steps_executed,
            execution_status: trans4mers_domain::state::ExecutionStatus::Running,
            execution_phase: trans4mers_domain::execution::ExecutionPhase::LlmGeneration,
            inbox_cursor: None,
            pending_tool_state: None,
            last_event_sequence: current_sequence_id,
            context_snapshot,
            created_at: chrono::Utc::now(),
        };
        db.with_write_tx(|conn| {
            crate::checkpoint_manager::CheckpointManager::save_checkpoint(conn, &cp)?;
            conn.execute(
                "UPDATE agent_executions SET generation = ?1, updated_at = ?2 WHERE id = ?3",
                rusqlite::params![
                    cp.generation,
                    chrono::Utc::now().to_rfc3339(),
                    state.execution_id.as_str()
                ],
            )?;
            Ok(())
        })
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

        if let Some(ref msg_id) = claimed_msg_id_opt {
            let _ = db.with_write_tx(|conn| {
                crate::agent_inbox::AgentInbox::ack_message(conn, msg_id, agent_id)
            });
        }

        // 1. Compact Context if it's getting too large
        let compaction_cfg = app_state.config.read().await.compaction.clone();
        tokio::select! {
            _ = ContextCompactor::compact_if_needed(&mut state, 20, provider.clone(), &model_config, &compaction_cfg) => {},
            _ = cancellation_token.cancelled() => return Err(Trans4mersError::Internal("Execution cancelled".to_string())),
        }

        // 2. Retrieve Semantic Memory (RAG)
        let (learned_rules, episodic_memories) = if let Some(mem) = memory_engine {
            let mut recent_thoughts = String::new();
            for step in state.steps.iter().rev().take(3) {
                recent_thoughts.push_str(&step.thought);
                recent_thoughts.push(' ');
            }
            let query_text = format!("{} {}", global_instructions, recent_thoughts);

            let query_embedding = provider.embed(&query_text, &model_config).await.ok();

            let rules = mem
                .retrieve_relevant_rules(
                    project_id,
                    agent_id,
                    Some(&query_text),
                    query_embedding.as_deref(),
                    5,
                )
                .unwrap_or_default();
            let eps = mem
                .retrieve_memories(
                    project_id,
                    agent_id,
                    None,
                    Some(&query_text),
                    query_embedding.as_deref(),
                    10,
                )
                .unwrap_or_default();
            (rules, eps)
        } else {
            (Vec::new(), Vec::new())
        };

        let executor = app_state
            .get_or_create_tool_executor(project_id, provider.clone())
            .await;

        // Extract tools dynamically for the LLM
        let native_tools: Vec<serde_json::Value> = {
            let registry = executor.registry.read().await;
            registry
                .values()
                .map(|t| {
                    let m = t.manifest();
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": m.name,
                            "description": m.description,
                            "parameters": m.input_schema
                        }
                    })
                })
                .collect()
        };

        let tools = if native_tools.is_empty() {
            None
        } else {
            Some(native_tools)
        };

        // Grammar-Constrained Decoding: Derive tool envelope schema if tools are present
        let response_schema = if let Some(ref tool_list) = tools {
            if !tool_list.is_empty() && provider.name() != "ollama" {
                let tool_names: Vec<serde_json::Value> = tool_list
                    .iter()
                    .filter_map(|t| t.get("function").and_then(|f| f.get("name")).cloned())
                    .collect();
                Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "thought": { "type": "string" },
                        "tool": { "type": "string", "enum": tool_names },
                        "arguments": { "type": "object" },
                        "response": { "type": "string" }
                    },
                    "required": ["thought"]
                }))
            } else {
                None
            }
        } else {
            None
        };

        let tool_name_strs: Vec<String> = {
            let registry = executor.registry.read().await;
            registry
                .values()
                .map(|t| t.manifest().name.clone())
                .collect()
        };

        let all_skills = app_state.skill_catalog.get_all_skills();
        let skills_slice = if all_skills.is_empty() {
            None
        } else {
            Some(all_skills.as_slice())
        };

        let messages = ContextEngine::assemble_prompt_with_context(
            &state,
            global_instructions,
            &learned_rules,
            &episodic_memories,
            Some(&tool_name_strs),
            None,
            skills_slice,
            model_config.context_limit.map(|v| v as usize),
        );

        let mut request = LlmRequest {
            messages,
            config: model_config.clone(),
            tools,
            response_schema,
        };

        let mut reserved_cost_usd = 0.0;
        // Cost Guard: Check budget ceilings before invoking LLM
        {
            let cost_guard = app_state.config.read().await.cost_guard.clone();
            if cost_guard.enabled && provider.name() != "ollama" {
                let _guard = BUDGET_LOCK.lock().await;

                let pid_str = project_id.to_string();
                let (daily_usd, monthly_usd) = app_state.global_db.with_read_conn(|conn| {
                    let d = trans4mers_storage::repos::cost_repo::get_daily_cost(
                        conn,
                        provider.name(),
                        Some(pid_str.as_str()),
                    )
                    .unwrap_or(0.0);
                    let m = trans4mers_storage::repos::cost_repo::get_monthly_cost(
                        conn,
                        provider.name(),
                        Some(pid_str.as_str()),
                    )
                    .unwrap_or(0.0);
                    Ok((d, m))
                })?;

                // Estimate based on typical use
                let estimated_cost_usd = 0.05;
                reserved_cost_usd = estimated_cost_usd;

                if let Some(daily_cap) = cost_guard.global_daily_ceiling_usd
                    && daily_usd + estimated_cost_usd >= daily_cap
                    && cost_guard.hard_block
                {
                    return Err(Trans4mersError::PolicyDenied {
                        capability: format!(
                            "Cost guard daily budget exceeded (${:.2} >= ${:.2}) for provider '{}'",
                            daily_usd,
                            daily_cap,
                            provider.name()
                        ),
                    });
                }
                if let Some(monthly_cap) = cost_guard.global_monthly_ceiling_usd
                    && monthly_usd + estimated_cost_usd >= monthly_cap
                    && cost_guard.hard_block
                {
                    return Err(Trans4mersError::PolicyDenied {
                        capability: format!(
                            "Cost guard monthly budget exceeded (${:.2} >= ${:.2}) for provider '{}'",
                            monthly_usd,
                            monthly_cap,
                            provider.name()
                        ),
                    });
                }

                let reserve_entry = trans4mers_domain::token_usage::CostEntry {
                    id: uuid::Uuid::new_v4().to_string(),
                    provider: provider.name().to_string(),
                    model: model_config.model.clone(),
                    project_id: Some(project_id.to_string()),
                    execution_id: Some(state.execution_id.to_string()),
                    input_tokens: 0,
                    output_tokens: 0,
                    cost_usd: estimated_cost_usd,
                    is_fallback: false,
                    created_at: chrono::Utc::now(),
                };
                let _ = app_state.global_db.with_write_tx(|conn| {
                    trans4mers_storage::repos::cost_repo::log_cost(conn, &reserve_entry)
                });
            }
        }

        let step_start_instant = std::time::Instant::now();
        let mut retry_count = 0;
        let response = loop {
            let stream_outcome: Result<trans4mers_domain::provider::LlmResponse, Trans4mersError> = {
                let stream_res = tokio::select! {
                    res = provider.generate_stream(&request) => res,
                    _ = cancellation_token.cancelled() => return Err(Trans4mersError::Internal("Execution cancelled".to_string())),
                };

                match stream_res {
                    Ok(mut stream) => {
                        use futures::StreamExt;
                        let mut accumulated_text = String::new();
                        let mut stream_metrics = None;
                        let mut tool_calls_map: std::collections::BTreeMap<
                            usize,
                            (Option<String>, Option<String>, String),
                        > = std::collections::BTreeMap::new();
                        let mut stream_error = None;

                        while let Some(chunk_res) = tokio::select! {
                            next_chunk = stream.next() => next_chunk,
                            _ = cancellation_token.cancelled() => {
                                return Err(Trans4mersError::Internal("Execution cancelled mid-stream".to_string()));
                            }
                        } {
                            match chunk_res {
                                Ok(chunk) => match chunk {
                                    trans4mers_domain::provider::LlmStreamChunk::TextDelta(delta) => {
                                        accumulated_text.push_str(&delta);
                                        let delta_event =
                                            trans4mers_domain::event::DomainEvent::TextDelta {
                                                execution_id: state.execution_id,
                                                delta,
                                            };
                                        let env = trans4mers_domain::event::EventEnvelope {
                                            sequence_id: current_sequence_id,
                                            event_id: trans4mers_domain::ids::EventId::new(),
                                            event: delta_event,
                                            actor_id: trans4mers_domain::ids::ActorId::from_uuid(
                                                *agent_id.as_uuid(),
                                            ),
                                            signature: None,
                                            created_at: chrono::Utc::now(),
                                        };
                                        let env_arc = std::sync::Arc::new(env);
                                        let _ = app_state
                                            .get_event_bus(project_id)
                                            .publish(env_arc.clone());
                                        let _ = app_state.global_event_bus.publish(env_arc);
                                    }
                                    trans4mers_domain::provider::LlmStreamChunk::ToolCallDelta {
                                        index,
                                        id,
                                        name,
                                        arguments_delta,
                                    } => {
                                        let entry = tool_calls_map.entry(index).or_insert((
                                            None,
                                            None,
                                            String::new(),
                                        ));
                                        if id.is_some() {
                                            entry.0 = id;
                                        }
                                        if name.is_some() {
                                            entry.1 = name;
                                        }
                                        entry.2.push_str(&arguments_delta);
                                    }
                                    trans4mers_domain::provider::LlmStreamChunk::Usage(metrics) => {
                                        stream_metrics = Some(metrics);
                                    }
                                    trans4mers_domain::provider::LlmStreamChunk::Finish {
                                        reason: _,
                                    } => {
                                        break;
                                    }
                                },
                                Err(e) => {
                                    stream_error = Some(e);
                                    break;
                                }
                            }
                        }

                        if let Some(e) = stream_error {
                            Err(e)
                        } else {
                            let reconstructed_tool_calls = if tool_calls_map.is_empty() {
                                None
                            } else {
                                let mut tc_vec = Vec::new();
                                for (idx, (id, name, args)) in tool_calls_map {
                                    let call_id = id.unwrap_or_else(|| format!("call_{}", idx));
                                    let fn_name = name.unwrap_or_default();
                                    tc_vec.push(serde_json::json!({
                                        "id": call_id,
                                        "type": "function",
                                        "function": {
                                            "name": fn_name,
                                            "arguments": args
                                        }
                                    }));
                                }
                                Some(tc_vec)
                            };

                            let metrics = stream_metrics.unwrap_or_else(|| {
                                let est_prompt = (request
                                    .messages
                                    .iter()
                                    .map(|m| m.content.len())
                                    .sum::<usize>()
                                    / 4) as u32;
                                let est_comp = (accumulated_text.len() / 4) as u32;
                                trans4mers_domain::token_usage::TokenMetrics {
                                    prompt_tokens: est_prompt,
                                    completion_tokens: est_comp,
                                    total_tokens: est_prompt + est_comp,
                                }
                            });

                            Ok(trans4mers_domain::provider::LlmResponse {
                                content: accumulated_text,
                                tool_calls: reconstructed_tool_calls,
                                metrics,
                            })
                        }
                    }
                    Err(e) => Err(e),
                }
            };

            match stream_outcome {
                Ok(resp) => break resp,
                Err(e) => {
                    let policy = SelfHealing::handle_error(&e, EffectClass::ReadOnly);
                    if !policy.can_retry || retry_count >= policy.max_retries {
                        return Err(e);
                    }
                    retry_count += 1;
                    if policy.backoff_ms > 0 {
                        tokio::select! {
                            _ = tokio::time::sleep(std::time::Duration::from_millis(policy.backoff_ms)) => {},
                            _ = cancellation_token.cancelled() => return Err(Trans4mersError::Internal("Execution cancelled".to_string())),
                        }
                    }
                    if policy.requires_context_compaction {
                        let compaction_cfg = app_state.config.read().await.compaction.clone();
                        tokio::select! {
                            _ = ContextCompactor::compact_if_needed(&mut state, 10, provider.clone(), &model_config, &compaction_cfg) => {},
                            _ = cancellation_token.cancelled() => return Err(Trans4mersError::Internal("Execution cancelled".to_string())),
                        }
                        // Re-assemble prompt using compacted state so retry doesn't send stale history
                        request.messages = ContextEngine::assemble_prompt_with_context(
                            &state,
                            global_instructions,
                            &learned_rules,
                            &episodic_memories,
                            Some(&tool_name_strs),
                            None,
                            skills_slice,
                            model_config.context_limit.map(|v| v as usize),
                        );
                    }
                }
            }
        };

        // Compute model-specific token cost
        let cost_usd = if provider.name() == "ollama" {
            0.0
        } else {
            let m = model_config.model.to_lowercase();
            let pricing_list = app_state.config.read().await.pricing.clone();
            let mut p_rate = 0.000001;
            let mut c_rate = 0.000003;
            for pricing in &pricing_list {
                if m.contains(&pricing.model_pattern) || pricing.model_pattern == "default" {
                    p_rate = pricing.prompt_cost_per_token;
                    c_rate = pricing.completion_cost_per_token;
                    if pricing.model_pattern != "default" {
                        break;
                    }
                }
            }
            (response.metrics.prompt_tokens as f32 * p_rate)
                + (response.metrics.completion_tokens as f32 * c_rate)
        };

        // Log actual cost into cost_entries (adjustment)
        {
            let cost_entry = trans4mers_domain::token_usage::CostEntry {
                id: uuid::Uuid::new_v4().to_string(),
                provider: provider.name().to_string(),
                model: model_config.model.clone(),
                project_id: Some(project_id.to_string()),
                execution_id: Some(state.execution_id.to_string()),
                input_tokens: response.metrics.prompt_tokens,
                output_tokens: response.metrics.completion_tokens,
                cost_usd: cost_usd - reserved_cost_usd,
                is_fallback: false,
                created_at: chrono::Utc::now(),
            };
            let _ = app_state.global_db.with_write_tx(|conn| {
                trans4mers_storage::repos::cost_repo::log_cost(conn, &cost_entry)
            });
        }

        let mut action: Option<serde_json::Value> = None;
        let mut thought = String::new();

        let registry_guard = executor.registry.read().await;

        if let Some(tool_calls) = response.tool_calls {
            // Authentic native tool calling support
            if let Some(tc) = tool_calls.first()
                && let Some(function) = tc.get("function")
            {
                let tool_name = function
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let arguments_str = function
                    .get("arguments")
                    .and_then(|v| v.as_str())
                    .unwrap_or("{}");
                let arguments: serde_json::Value =
                    serde_json::from_str(arguments_str).unwrap_or(serde_json::json!({}));

                if registry_guard.contains_key(&tool_name) {
                    action = Some(serde_json::json!({
                        "tool_name": tool_name,
                        "arguments": arguments
                    }));
                    thought = if response.content.trim().is_empty() {
                        format!("Calling tool `{}`", tool_name)
                    } else {
                        response.content.trim().to_string()
                    };
                } else {
                    thought = response.content.trim().to_string();
                }
            }
        } else {
            // Robust extraction for models returning tool calls or JSON inside content
            let start = response.content.find('{');
            let end = response.content.rfind('}');

            if let (Some(s), Some(e)) = (start, end) {
                if s < e {
                    let json_str = &response.content[s..=e];
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str) {
                        // Check for function call style: {"tool": "...", "arguments": ...} or {"name": "...", "arguments": ...}
                        if let Some(tool_name) = parsed
                            .get("tool")
                            .or_else(|| parsed.get("name"))
                            .or_else(|| parsed.get("tool_name"))
                            .and_then(|v| v.as_str())
                        {
                            if registry_guard.contains_key(tool_name) {
                                let arguments = parsed
                                    .get("arguments")
                                    .or_else(|| parsed.get("parameters"))
                                    .cloned()
                                    .unwrap_or(serde_json::json!({}));
                                action = Some(serde_json::json!({
                                    "tool_name": tool_name,
                                    "arguments": arguments
                                }));
                                thought = parsed
                                    .get("thought")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or(&format!("Calling tool `{}`", tool_name))
                                    .to_string();
                            } else {
                                thought = response.content.clone();
                            }
                        } else if let Some(act_val) = parsed.get("action") {
                            if let Some(tool_name) =
                                act_val.get("tool_name").and_then(|v| v.as_str())
                            {
                                if registry_guard.contains_key(tool_name) {
                                    action = Some(act_val.clone());
                                    thought = parsed
                                        .get("thought")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                } else {
                                    thought = response.content.clone();
                                }
                            } else if act_val.as_str() == Some("Finished") {
                                action = Some(serde_json::json!("Finished"));
                                thought = parsed
                                    .get("thought")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                            } else {
                                thought = response.content.clone();
                            }
                        } else if let Some(res) = parsed.get("response").and_then(|v| v.as_str()) {
                            thought = res.to_string();
                        } else if let Some(th) = parsed.get("thought").and_then(|v| v.as_str()) {
                            thought = th.to_string();
                        } else {
                            thought = response.content.clone();
                        }
                    } else {
                        thought = response.content.clone();
                    }
                } else {
                    thought = response.content.clone();
                }
            } else {
                thought = response.content.clone();
            }
        }
        drop(registry_guard);

        if thought.trim().is_empty() {
            if action.is_some() {
                thought = "Executing tool action.".to_string();
            } else {
                thought = "I have reviewed your request.".to_string();
            }
        }

        let mut step = ReActStep {
            step_index: state.steps.len() as u32,
            thought: thought.to_string(),
            action_intent: action.clone(),
            result_payload: None,
            error: None,
            token_usage: Some(response.metrics.clone()),
            created_at: chrono::Utc::now(),
            completed_at: None,
        };

        // CHUNK 1 FIX: Actually persist token usage so it is billed!
        {
            let usage = trans4mers_domain::token_usage::TokenUsage {
                id: trans4mers_domain::ids::TokenUsageId::new(),
                execution_id: state.execution_id,
                agent_instance_id: *agent_id,
                conversation_id: state.conversation_id, // Assuming state has it
                provider: provider.name().to_string(),
                model: model_config.model.clone(),
                prompt_tokens: response.metrics.prompt_tokens,
                completion_tokens: response.metrics.completion_tokens,
                total_tokens: response.metrics.total_tokens,
                estimated_cost_usd: Some(cost_usd as f64),
                compute_time_ms: step_start_instant.elapsed().as_millis() as u64,
                created_at: chrono::Utc::now(),
            };

            let db = app_state
                .get_project_db(project_id)
                .ok_or_else(|| Trans4mersError::Internal("Project DB not found".to_string()))?;
            db.with_write_tx(|tx| {
                trans4mers_storage::repos::token_usage_repo::insert_token_usage(tx, &usage)
            })?;
        }

        let is_finished = if let Some(act) = &action {
            act.as_str().map(|s| s == "Finished").unwrap_or(false)
        } else {
            // The LLM produced an answer/thought without requesting any tool action.
            // Reasoning is complete; do not loop aimlessly.
            true
        };

        // 6. Tool Execution Logic (Zero-Trust sandbox enforced)
        if let Some(act) = &action
            && let Some(tool_name) = act.get("tool_name").and_then(|v| v.as_str())
        {
            let workspace_root = if let Some(path) = &workspace_path_override {
                Some(path.clone())
            } else {
                let mut root = None;
                let _ = app_state.global_db.with_read_conn(|conn| {
                    if let Ok(Some(proj)) =
                        trans4mers_storage::repos::project_repo::get_project(conn, project_id)
                    {
                        let base_path = std::path::PathBuf::from(proj.workspace_path);
                        let worktree_path =
                            base_path.join(format!(".trans4mers/worktrees/{}", agent_id));
                        if worktree_path.exists() {
                            root = Some(worktree_path);
                        } else {
                            root = Some(base_path);
                        }
                    }
                    Ok(())
                });
                root
            };

            let workspace_root = match workspace_root {
                Some(r) => r,
                None => {
                    step.error = Some(trans4mers_domain::execution::StepError {
                        error_type: "WorkspaceError".to_string(),
                        message: "Project workspace could not be resolved from DB.".to_string(),
                        stack_trace: None,
                    });
                    state.steps.push(step);
                    return Err(Trans4mersError::Internal(
                        "Workspace root not found".to_string(),
                    ));
                }
            };

            let tool_request = trans4mers_domain::tool::ToolRequest {
                tool_name: tool_name.to_string(),
                arguments: act
                    .get("arguments")
                    .cloned()
                    .unwrap_or(serde_json::json!({})),
                requesting_agent_id: *agent_id,
                project_id: *project_id,
                execution_id: state.execution_id,
                workspace_root: workspace_root.clone(),
                idempotency_key: None,
            };

            let manifest_opt = {
                let registry = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current()
                        .block_on(async { executor.registry.read().await.clone() })
                });
                registry
                    .get(&tool_request.tool_name)
                    .map(|t| t.manifest().clone())
            };

            let manifest = match manifest_opt {
                Some(m) => m,
                None => {
                    step.error = Some(trans4mers_domain::execution::StepError {
                        error_type: "ToolNotFound".to_string(),
                        message: format!("Tool {} is not registered", tool_request.tool_name),
                        stack_trace: None,
                    });
                    state.steps.push(step.clone());
                    break;
                }
            };

            // DIFF REVIEW & SECRET SCANNING FOR FILE MUTATIONS
            let mut diff_auto_approved = false;
            let mut diff_forced_approval_id: Option<String> = None;
            if tool_request.tool_name == "filesystem.write"
                && let (Some(rel_path), Some(new_content)) = (
                    tool_request.arguments.get("path").and_then(|v| v.as_str()),
                    tool_request
                        .arguments
                        .get("content")
                        .and_then(|v| v.as_str()),
                )
            {
                let old_text =
                    trans4mers_storage::filesystem::FileSystemGuard::read_workspace_file(
                        &workspace_root,
                        rel_path,
                    )
                    .unwrap_or_default();
                let (hunks, adds, dels) =
                    crate::diff_reviewer::DiffReviewer::compute_file_hunks(&old_text, new_content);
                let diff_cfg = app_state.config.read().await.diff_review.clone();
                let target_approval_id = uuid::Uuid::new_v4().to_string();
                let approval_id_typed =
                    trans4mers_domain::ids::ApprovalId::from_str(&target_approval_id).ok();

                if let Some(db) = app_state.get_project_db(project_id) {
                    let proposed = db.with_write_tx(|conn| {
                        crate::diff_reviewer::DiffReviewer::propose(
                            conn,
                            *project_id,
                            Some(state.execution_id),
                            Some(*agent_id),
                            "FilesystemWrite".to_string(),
                            "Medium".to_string(),
                            DiffKind::FilePatch {
                                file_path: rel_path.to_string(),
                                additions: adds,
                                deletions: dels,
                                is_new: old_text.is_empty(),
                                is_deleted: false,
                            },
                            new_content.to_string(),
                            hunks,
                            approval_id_typed,
                            &diff_cfg,
                        )
                    });

                    if let Ok(diff) = proposed {
                        if diff.decision == DiffDecision::AutoApproved {
                            diff_auto_approved = true;
                        } else {
                            diff_forced_approval_id = Some(target_approval_id);
                        }
                    }
                }
            } else if tool_request.tool_name == "memory.insert"
                && let Some(new_content) = tool_request
                    .arguments
                    .get("content")
                    .and_then(|v| v.as_str())
            {
                let tier = tool_request
                    .arguments
                    .get("tier")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Working");
                let importance = tool_request
                    .arguments
                    .get("importance")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.7) as f32;
                let diff_cfg = app_state.config.read().await.diff_review.clone();
                let target_approval_id = uuid::Uuid::new_v4().to_string();
                let approval_id_typed =
                    trans4mers_domain::ids::ApprovalId::from_str(&target_approval_id).ok();

                if let Some(db) = app_state.get_project_db(project_id) {
                    let proposed = db.with_write_tx(|conn| {
                        crate::diff_reviewer::DiffReviewer::propose(
                            conn,
                            *project_id,
                            Some(state.execution_id),
                            Some(*agent_id),
                            "MemoryInsert".to_string(),
                            "Low".to_string(),
                            DiffKind::MemoryMutation {
                                old_content: None,
                                new_content: new_content.to_string(),
                                tier: tier.to_string(),
                                importance,
                            },
                            new_content.to_string(),
                            vec![],
                            approval_id_typed,
                            &diff_cfg,
                        )
                    });

                    if let Ok(diff) = proposed {
                        if diff.decision == DiffDecision::AutoApproved {
                            diff_auto_approved = true;
                        } else {
                            diff_forced_approval_id = Some(target_approval_id);
                        }
                    }
                }
            } else if tool_request.tool_name == "memory.replace"
                && let (Some(old_mem_id), Some(new_content)) = (
                    tool_request
                        .arguments
                        .get("old_memory_id")
                        .and_then(|v| v.as_str()),
                    tool_request
                        .arguments
                        .get("new_content")
                        .and_then(|v| v.as_str()),
                )
            {
                let tier = tool_request
                    .arguments
                    .get("tier")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Semantic");
                let importance = tool_request
                    .arguments
                    .get("importance")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.8) as f32;
                let old_text = if let Some(db) = app_state.get_project_db(project_id) {
                    db.with_read_conn(|conn| {
                        trans4mers_storage::repos::memory_repo::get_project_memory(conn, old_mem_id)
                    })
                    .ok()
                    .flatten()
                    .map(|m| m.content)
                } else {
                    None
                };

                let diff_cfg = app_state.config.read().await.diff_review.clone();
                let target_approval_id = uuid::Uuid::new_v4().to_string();
                let approval_id_typed =
                    trans4mers_domain::ids::ApprovalId::from_str(&target_approval_id).ok();

                if let Some(db) = app_state.get_project_db(project_id) {
                    let proposed = db.with_write_tx(|conn| {
                        crate::diff_reviewer::DiffReviewer::propose(
                            conn,
                            *project_id,
                            Some(state.execution_id),
                            Some(*agent_id),
                            "MemoryReplace".to_string(),
                            "Medium".to_string(),
                            DiffKind::MemoryMutation {
                                old_content: old_text,
                                new_content: new_content.to_string(),
                                tier: tier.to_string(),
                                importance,
                            },
                            new_content.to_string(),
                            vec![],
                            approval_id_typed,
                            &diff_cfg,
                        )
                    });

                    if let Ok(diff) = proposed {
                        if diff.decision == DiffDecision::AutoApproved {
                            diff_auto_approved = true;
                        } else {
                            diff_forced_approval_id = Some(target_approval_id);
                        }
                    }
                }
            }

            // ZERO-TRUST POLICY ENFORCEMENT
            use trans4mers_domain::policy::PolicyOutcome;
            let policy_outcome = if diff_auto_approved {
                PolicyOutcome::Allow
            } else if diff_forced_approval_id.is_some() {
                PolicyOutcome::Ask
            } else {
                PolicyEngine::evaluate(&tool_request, agent_id, project_id, &app_state)?
            };

            match policy_outcome {
                PolicyOutcome::Deny => {
                    step.error = Some(trans4mers_domain::execution::StepError {
                        error_type: "PolicyViolation".to_string(),
                        message: "Agent attempted to execute an unauthorized tool".to_string(),
                        stack_trace: None,
                    });
                }
                PolicyOutcome::Ask => {
                    let db = app_state.get_project_db(project_id).ok_or_else(|| {
                        Trans4mersError::Database("Project DB not found".to_string())
                    })?;
                    let already_approved = db
                        .with_read_conn(|conn| {
                            crate::approval_engine::ApprovalEngine::has_approved(
                                conn,
                                &state.execution_id,
                                &tool_request.tool_name,
                                &tool_request.arguments,
                            )
                        })
                        .unwrap_or(false);

                    let is_browser_action = tool_request.tool_name.starts_with("browser.");
                    let browser_space_id = tool_request
                        .arguments
                        .get("space_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("bspace_default");

                    let pre_action_snapshot = if is_browser_action && !already_approved {
                        let space =
                            crate::browser_space_manager::BrowserSpaceManager::resolve_space(
                                &app_state,
                                project_id,
                                browser_space_id,
                            );
                        crate::browser_space_manager::BrowserSpaceManager::take_snapshot(
                            &app_state,
                            project_id,
                            &space,
                            Some(&format!(
                                "Pre-approval snapshot before {}",
                                tool_request.tool_name
                            )),
                        )
                        .ok()
                    } else {
                        None
                    };

                    let approved = if already_approved {
                        true
                    } else {
                        // Pause execution, drop permit while waiting for approval
                        drop(current_permit.take());

                        let approval_id = diff_forced_approval_id
                            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                        let capability = manifest.required_capabilities.first().cloned().unwrap_or(
                            trans4mers_domain::tool::Capability::Custom("Unknown".to_string()),
                        );
                        let event_bus = app_state.get_event_bus(project_id);
                        let args_hash =
                            Some(crate::approval_engine::ApprovalEngine::hash_arguments(
                                &tool_request.arguments,
                            ));

                        // Properly use CQRS to commit the event and project it synchronously
                        let approval_envelope = {
                            db.with_write_tx(|tx| {
                                crate::approval_engine::ApprovalEngine::request_approval(
                                    tx,
                                    approval_id.clone(),
                                    state.execution_id,
                                    *agent_id,
                                    state.conversation_id,
                                    capability,
                                    tool_request.tool_name.clone(),
                                    serde_json::to_string(&tool_request.arguments)
                                        .unwrap_or_default(),
                                    args_hash,
                                    manifest.baseline_risk,
                                )
                            })
                            .map_err(|e| Trans4mersError::Database(e.to_string()))?
                        };
                        // Subscribe before publishing to ensure no broadcast event is missed
                        let mut rx = event_bus.subscribe();

                        let env_arc = std::sync::Arc::new(approval_envelope);
                        let _ = event_bus.publish(env_arc.clone());
                        let _ = app_state.global_event_bus.publish(env_arc);

                        // Check if approval was already resolved synchronously before subscription
                        let is_app = if let Some(resolved_status) = db
                            .with_read_conn(|conn| {
                                let res: Result<String, _> = conn.query_row(
                                    "SELECT status FROM approvals WHERE id = ?1",
                                    rusqlite::params![approval_id.as_str()],
                                    |row| row.get(0),
                                );
                                match res {
                                    Ok(s) if s == "Approved" => Ok(Some(true)),
                                    Ok(s) if s == "Denied" || s == "Rejected" => Ok(Some(false)),
                                    _ => Ok(None),
                                }
                            })
                            .unwrap_or(None)
                        {
                            resolved_status
                        } else {
                            loop {
                                tokio::select! {
                                    _ = cancellation_token.cancelled() => return Err(Trans4mersError::Internal("Execution cancelled".to_string())),
                                    recv_res = rx.recv() => {
                                        match recv_res {
                                            Ok(env) => {
                                                if let trans4mers_domain::event::DomainEvent::ApprovalResolved { approval_id: resolved_id, approved: is_approved, .. } = &env.event
                                                    && resolved_id == &approval_id {
                                                        break *is_approved;
                                                    }
                                            }
                                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                                                // Buffer lagged: check DB directly to verify if approval was resolved
                                                if let Some(db) = app_state.get_project_db(project_id) {
                                                    let status = db.with_read_conn(|conn| {
                                                        let res: Result<String, _> = conn.query_row(
                                                            "SELECT status FROM approvals WHERE id = ?1",
                                                            rusqlite::params![approval_id.as_str()],
                                                            |row| row.get(0),
                                                        );
                                                        match res {
                                                            Ok(s) if s == "Approved" => Ok(Some(true)),
                                                            Ok(s) if s == "Denied" || s == "Rejected" => Ok(Some(false)),
                                                            _ => Ok(None),
                                                        }
                                                    }).unwrap_or(None);
                                                    if let Some(approved) = status {
                                                        break approved;
                                                    }
                                                }
                                            }
                                            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                                return Err(Trans4mersError::Internal("Event bus closed while waiting for approval".to_string()));
                                            }
                                        }
                                    }
                                }
                            }
                        };

                        // Re-acquire permit before resuming execution
                        current_permit = Some(app_state.scheduler.acquire_permit().await?);
                        is_app
                    };

                    if !approved {
                        if let (true, Some(snap)) = (is_browser_action, pre_action_snapshot) {
                            let _ = crate::browser_space_manager::BrowserSpaceManager::rollback_to_snapshot(
                                    app_state.clone(),
                                    project_id,
                                    browser_space_id,
                                    &snap.id,
                                ).await;
                        }
                        step.error = Some(trans4mers_domain::execution::StepError {
                            error_type: "ApprovalDenied".to_string(),
                            message: "Human explicitly denied the action".to_string(),
                            stack_trace: None,
                        });
                    } else {
                        // Execute tool after approval with self-healing retry loop
                        let mut retry_count = 0;
                        let tool_res = loop {
                            let res = tokio::select! {
                                r = executor.execute_tool(&tool_request) => r,
                                _ = cancellation_token.cancelled() => return Err(Trans4mersError::Internal("Execution cancelled".to_string())),
                            };

                            match res {
                                Ok(result) => break Ok(result),
                                Err(e) => {
                                    let policy =
                                        SelfHealing::handle_error(&e, manifest.effect_class);
                                    if !policy.can_retry || retry_count >= policy.max_retries {
                                        break Err(e);
                                    }
                                    retry_count += 1;
                                    if policy.backoff_ms > 0 {
                                        tokio::select! {
                                            _ = tokio::time::sleep(std::time::Duration::from_millis(policy.backoff_ms)) => {},
                                            _ = cancellation_token.cancelled() => return Err(Trans4mersError::Internal("Execution cancelled".to_string())),
                                        }
                                    }
                                }
                            }
                        };

                        match tool_res {
                            Ok(result) => {
                                if !result.success {
                                    let err_msg = result.error.unwrap_or_else(|| {
                                        if result.content.trim().is_empty() {
                                            "Tool execution failed without output".to_string()
                                        } else {
                                            result.content.clone()
                                        }
                                    });
                                    step.error = Some(trans4mers_domain::execution::StepError {
                                        error_type: "ToolExecutionFailed".to_string(),
                                        message: err_msg.clone(),
                                        stack_trace: None,
                                    });
                                    step.result_payload = Some(serde_json::Value::String(format!(
                                        "Error: {}",
                                        err_msg
                                    )));
                                } else {
                                    step.result_payload =
                                        Some(serde_json::Value::String(result.content));
                                }
                            }
                            Err(e) => {
                                step.error = Some(trans4mers_domain::execution::StepError {
                                    error_type: "ToolExecutionError".to_string(),
                                    message: e.to_string(),
                                    stack_trace: None,
                                })
                            }
                        }
                    }
                }
                PolicyOutcome::Allow => {
                    // Self-healing retry loop
                    let mut retry_count = 0;
                    let tool_res = loop {
                        let res = tokio::select! {
                            r = executor.execute_tool(&tool_request) => r,
                            _ = cancellation_token.cancelled() => return Err(Trans4mersError::Internal("Execution cancelled".to_string())),
                        };

                        match res {
                            Ok(result) => break Ok(result),
                            Err(e) => {
                                let policy = SelfHealing::handle_error(&e, manifest.effect_class);
                                if !policy.can_retry || retry_count >= policy.max_retries {
                                    break Err(e);
                                }
                                retry_count += 1;
                                if policy.backoff_ms > 0 {
                                    tokio::select! {
                                        _ = tokio::time::sleep(std::time::Duration::from_millis(policy.backoff_ms)) => {},
                                        _ = cancellation_token.cancelled() => return Err(Trans4mersError::Internal("Execution cancelled".to_string())),
                                    }
                                }
                            }
                        }
                    };

                    match tool_res {
                        Ok(result) => {
                            if !result.success {
                                let err_msg = result.error.unwrap_or_else(|| {
                                    if result.content.trim().is_empty() {
                                        "Tool execution failed without output".to_string()
                                    } else {
                                        result.content.clone()
                                    }
                                });
                                step.error = Some(trans4mers_domain::execution::StepError {
                                    error_type: "ToolExecutionFailed".to_string(),
                                    message: err_msg.clone(),
                                    stack_trace: None,
                                });
                                step.result_payload =
                                    Some(serde_json::Value::String(format!("Error: {}", err_msg)));
                            } else {
                                step.result_payload =
                                    Some(serde_json::Value::String(result.content));
                            }
                        }
                        Err(e) => {
                            let policy = SelfHealing::handle_error(&e, manifest.effect_class);
                            if policy.fallback_to_delegation {
                                tracing::warn!(
                                    "Self-healing: Tool execution error in `{}`, escalating failure...",
                                    manifest.name
                                );
                                if let Some(pdb) = app_state.get_project_db(project_id) {
                                    let _ = pdb.with_write_tx(|conn| {
                                            if let Ok(parent_id_str) = conn.query_row(
                                                "SELECT parent_instance_id FROM agent_instances WHERE id = ?1",
                                                rusqlite::params![agent_id.as_str()],
                                                |row| row.get::<_, Option<String>>(0)
                                            )
                                                && let Some(pid) = parent_id_str
                                                    && let Ok(parent_agent_id) = trans4mers_domain::ids::AgentInstanceId::from_str(&pid) {
                                                        let escalation_payload = serde_json::json!({
                                                            "type": "Escalation",
                                                            "child_agent_id": agent_id.as_str(),
                                                            "tool_name": manifest.name,
                                                            "error": e.to_string(),
                                                        });
                                                        let msg_id = trans4mers_domain::ids::InboxMessageId::new();
                                                        let _ = crate::agent_inbox::AgentInbox::queue_message(
                                                            conn,
                                                            &msg_id,
                                                            &parent_agent_id,
                                                            &agent_id.to_string(),
                                                            &escalation_payload,
                                                        );
                                                    }
                                            Ok(())
                                        });
                                }
                            }
                            step.error = Some(trans4mers_domain::execution::StepError {
                                error_type: "ToolExecutionError".to_string(),
                                message: e.to_string(),
                                stack_trace: None,
                            });
                        }
                    }
                }
            }
        }
        step.completed_at = Some(chrono::Utc::now());
        state.total_steps_executed += 1;
        step.step_index = state.total_steps_executed;
        state.steps.push(step.clone());

        // CQRS: Emit step events
        {
            let db = app_state
                .get_project_db(project_id)
                .ok_or_else(|| Trans4mersError::Database("Project DB not found".to_string()))?;
            let envelopes = db
                .with_write_tx(|conn| {
                    let mut envs = Vec::new();

                    // If a tool was executed this step
                    if let Some(act) = &step.action_intent
                        && let Some(tool_name) = act.get("tool_name").and_then(|v| v.as_str())
                    {
                        let tool_event = trans4mers_domain::event::DomainEvent::ToolExecuted {
                            execution_id: state.execution_id,
                            tool_name: tool_name.to_string(),
                            success: step.error.is_none(),
                        };
                        let env1 = crate::cqrs::commit_event(
                            conn,
                            tool_event,
                            trans4mers_domain::ids::ActorId::from_uuid(*agent_id.as_uuid()),
                        )?;
                        envs.push(env1);
                    }

                    let step_event =
                        trans4mers_domain::event::DomainEvent::ExecutionStepCompleted {
                            execution_id: state.execution_id,
                            step_number: state.total_steps_executed,
                        };
                    let env2 = crate::cqrs::commit_event(
                        conn,
                        step_event,
                        trans4mers_domain::ids::ActorId::from_uuid(*agent_id.as_uuid()),
                    )?;
                    envs.push(env2);
                    Ok(envs)
                })
                .map_err(|e| Trans4mersError::Database(e.to_string()))?;

            for env in envelopes {
                current_sequence_id = env.sequence_id;
                let env_arc = std::sync::Arc::new(env);
                let _ = app_state.get_event_bus(project_id).publish(env_arc.clone());
                let _ = app_state.global_event_bus.publish(env_arc);
            }
        }

        let repeated_error = if state.steps.len() >= 2 {
            let last = &state.steps[state.steps.len() - 1];
            let prev = &state.steps[state.steps.len() - 2];
            match (&last.error, &prev.error) {
                (Some(e1), Some(e2)) => e1.message == e2.message && !e1.message.is_empty(),
                _ => false,
            }
        } else {
            false
        };

        let repeated_action = if state.steps.len() >= 2 {
            let last = &state.steps[state.steps.len() - 1];
            let prev = &state.steps[state.steps.len() - 2];
            match (&last.action_intent, &prev.action_intent) {
                (Some(a1), Some(a2)) => a1 == a2,
                _ => false,
            }
        } else {
            false
        };

        let is_completed_tool = if let Some(act) = &step.action_intent {
            act.get("tool_name").and_then(|v| v.as_str()) == Some("complete_task")
                && step.error.is_none()
        } else {
            false
        };

        if is_finished
            || is_completed_tool
            || state.total_steps_executed >= 25
            || repeated_error
            || repeated_action
        {
            if repeated_action {
                tracing::info!(
                    "Agent loop breaker: consecutive identical tool action detected, terminating loop to synthesize."
                );
            }
            break;
        }
    }

    let has_repeated_error = if state.steps.len() >= 2 {
        let last = &state.steps[state.steps.len() - 1];
        let prev = &state.steps[state.steps.len() - 2];
        match (&last.error, &prev.error) {
            (Some(e1), Some(e2)) => e1.message == e2.message && !e1.message.is_empty(),
            _ => false,
        }
    } else {
        false
    };

    let is_failure = has_repeated_error
        || (state.total_steps_executed >= 25
            && state.steps.last().and_then(|s| s.error.as_ref()).is_some());
    let final_status = if is_failure {
        trans4mers_domain::state::ExecutionStatus::Failed
    } else {
        trans4mers_domain::state::ExecutionStatus::Completed
    };

    // Save final durable checkpoint before exiting so all executed steps are persisted in telemetry
    {
        if let Some(db) = app_state.get_project_db(project_id) {
            let context_snapshot = serde_json::to_value(&state).ok();
            let cp = trans4mers_domain::execution::Checkpoint {
                id: trans4mers_domain::ids::CheckpointId::new(),
                execution_id: state.execution_id,
                generation: state.total_steps_executed as u64,
                step_number: state.total_steps_executed,
                execution_status: final_status,
                execution_phase: trans4mers_domain::execution::ExecutionPhase::LlmGeneration,
                inbox_cursor: None,
                pending_tool_state: None,
                last_event_sequence: current_sequence_id,
                context_snapshot,
                created_at: chrono::Utc::now(),
            };
            let _ = db.with_write_tx(|conn| {
                crate::checkpoint_manager::CheckpointManager::save_checkpoint(conn, &cp)?;
                let _ = conn.execute(
                    "UPDATE agent_executions SET status = ?1, generation = ?2, updated_at = ?3 WHERE id = ?4",
                    rusqlite::params![
                        final_status.to_string(),
                        cp.generation,
                        chrono::Utc::now().to_rfc3339(),
                        state.execution_id.as_str()
                    ],
                );
                Ok(())
            });
        }
    }

    // Check if the agent already communicated directly via message.send tool
    let already_sent_message = state.steps.iter().any(|s| {
        s.action_intent
            .as_ref()
            .and_then(|a| a.get("tool_name"))
            .and_then(|t| t.as_str())
            == Some("message.send")
            && s.error.is_none()
    });

    if !already_sent_message {
        // Extract the final conversational reply to the channel
        let final_reply = state
            .steps
            .iter()
            .rev()
            .find_map(|step| {
                let t = step.thought.trim();
                if !t.is_empty()
                    && !t.starts_with("User Instruction from")
                    && !t.starts_with("I received a new message")
                {
                    Some(t.to_string())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| {
                if is_failure {
                    "Task execution failed due to errors.".to_string()
                } else {
                    "Completed requested task.".to_string()
                }
            });

        // CQRS: Commit MessageSent from the agent to the conversation channel
        {
            let agent_msg = trans4mers_domain::message::Message {
                id: trans4mers_domain::ids::MessageId::new(),
                conversation_id: target_convo_id,
                channel_id: target_channel_id,
                thread_id: None,
                sender: trans4mers_domain::actor::Actor::Agent(*agent_id),
                content: final_reply,
                message_kind: trans4mers_domain::message::MessageKind::Chat,
                mentions: vec![],
                attachments: vec![],
                requires_approval: false,
                approval_id: None,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            };

            let msg_event =
                trans4mers_domain::event::DomainEvent::MessageCreated { message: agent_msg };

            let db = app_state
                .get_project_db(project_id)
                .ok_or_else(|| Trans4mersError::Database("Project DB not found".to_string()))?;
            let envelope = db
                .with_write_tx(|conn| {
                    crate::cqrs::commit_event(
                        conn,
                        msg_event,
                        trans4mers_domain::ids::ActorId::from_uuid(*agent_id.as_uuid()),
                    )
                })
                .map_err(|e| Trans4mersError::Database(e.to_string()))?;

            let env_arc = std::sync::Arc::new(envelope);
            let _ = app_state.get_event_bus(project_id).publish(env_arc.clone());
            let _ = app_state.global_event_bus.publish(env_arc);
        }
    }

    // CQRS: Emit ExecutionCompleted or ExecutionFailed
    {
        let event = if is_failure {
            let reason = state
                .steps
                .last()
                .and_then(|s| s.error.as_ref())
                .map(|e| e.message.clone())
                .unwrap_or_else(|| {
                    "Execution terminated due to repeated errors or step limit".to_string()
                });
            trans4mers_domain::event::DomainEvent::ExecutionFailed {
                execution_id: state.execution_id,
                reason,
            }
        } else {
            trans4mers_domain::event::DomainEvent::ExecutionCompleted {
                execution_id: state.execution_id,
            }
        };
        let db = app_state
            .get_project_db(project_id)
            .ok_or_else(|| Trans4mersError::Database("Project DB not found".to_string()))?;
        let envelope = db
            .with_write_tx(|conn| {
                crate::cqrs::commit_event(
                    conn,
                    event,
                    trans4mers_domain::ids::ActorId::from_uuid(*agent_id.as_uuid()),
                )
            })
            .map_err(|e| Trans4mersError::Database(e.to_string()))?;
        let env_arc = std::sync::Arc::new(envelope);
        let _ = app_state.get_event_bus(project_id).publish(env_arc.clone());
        let _ = app_state.global_event_bus.publish(env_arc);
    }

    // Merge agent worktree branch back if one was used and cleanup
    if workspace_path_override.is_some() {
        let _ = app_state.global_db.with_read_conn(|conn| {
            if let Ok(Some(proj)) =
                trans4mers_storage::repos::project_repo::get_project(conn, project_id)
            {
                let base_path = std::path::PathBuf::from(proj.workspace_path);
                let branch_name = format!("agent/{}", agent_id);
                match crate::git_workspace::GitWorkspace::merge_agent_branch(
                    &base_path,
                    &branch_name,
                ) {
                    Ok(_) => {
                        tracing::info!(
                            "Successfully merged agent worktree branch {} into main",
                            branch_name
                        );
                        let _ = crate::git_workspace::GitWorkspace::cleanup_agent_worktree(
                            &base_path,
                            &agent_id.to_string(),
                        );
                    }
                    Err(e) => {
                        tracing::error!(
                            "Failed to merge agent worktree branch {}: {}. Worktree retained.",
                            branch_name,
                            e
                        );
                    }
                }
            }
            Ok(())
        });
    }

    Ok(state)
}
