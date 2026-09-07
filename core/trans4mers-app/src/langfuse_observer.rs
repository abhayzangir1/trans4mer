use std::sync::Arc;
use tracing::{debug, info, warn};
use trans4mers_domain::event::DomainEvent;
use trans4mers_domain::ids::ExecutionId;
use trans4mers_domain::langfuse::LangfuseConfig;
use trans4mers_engine::app_state::AppState;
use trans4mers_providers::langfuse::{
    IngestionBatch, LangfuseClient, create_generation_item, create_span_item, create_trace_item,
    sanitize_payload,
};
use trans4mers_storage::repos::{execution_repo, project_repo, settings_repo, token_usage_repo};

pub struct LangfuseObserver;

impl LangfuseObserver {
    /// Loads the active Langfuse configuration from global app settings and the OS Keyring.
    pub fn load_config(state: &AppState) -> LangfuseConfig {
        let (enabled, host, capture_prompts) = state
            .global_db
            .with_read_conn(|conn| {
                let enabled_str =
                    settings_repo::get_app_setting(conn, "langfuse_enabled")?.unwrap_or_default();
                let host_str =
                    settings_repo::get_app_setting(conn, "langfuse_host")?.unwrap_or_default();
                let capture_str = settings_repo::get_app_setting(conn, "langfuse_capture_prompts")?
                    .unwrap_or_default();

                Ok((
                    enabled_str == "true",
                    if host_str.trim().is_empty() {
                        "http://localhost:3000".to_string()
                    } else {
                        host_str
                    },
                    capture_str == "true",
                ))
            })
            .unwrap_or((false, "http://localhost:3000".to_string(), false));

        let public_key = crate::settings::KeyringManager::get_api_key("langfuse:public_key")
            .or_else(|_| crate::settings::KeyringManager::get_api_key("langfuse_public_key"))
            .ok();

        let secret_key = crate::settings::KeyringManager::get_api_key("langfuse:secret_key")
            .or_else(|_| crate::settings::KeyringManager::get_api_key("langfuse_secret_key"))
            .ok();

        LangfuseConfig {
            host,
            enabled,
            capture_prompts,
            public_key,
            secret_key,
        }
    }

    /// Spawns the background listener task that listens to the global EventBus.
    pub fn spawn(state: Arc<AppState>) {
        let mut rx = state.global_event_bus.subscribe();
        let state_clone = state.clone();

        tokio::spawn(async move {
            info!("LangfuseObserver background worker started.");
            while let Ok(envelope) = rx.recv().await {
                match &envelope.event {
                    DomainEvent::ExecutionCompleted { execution_id }
                    | DomainEvent::ExecutionFailed { execution_id, .. } => {
                        let exec_id = *execution_id;
                        let st = state_clone.clone();
                        tokio::spawn(async move {
                            if let Err(e) = Self::process_execution(&st, &exec_id).await {
                                warn!(
                                    "LangfuseObserver failed to export execution {}: {}",
                                    exec_id, e
                                );
                            }
                        });
                    }
                    _ => {}
                }
            }
        });
    }

    /// Processes an execution by finding its database, querying its steps and token usages,
    /// and sending the batch to Langfuse if enabled.
    pub async fn process_execution(
        state: &AppState,
        execution_id: &ExecutionId,
    ) -> Result<(), trans4mers_domain::error::Trans4mersError> {
        let config = Self::load_config(state);

        // HARD ZERO-EGRESS GUARD: If Langfuse is disabled or unset, ZERO network calls!
        if !config.enabled || config.host.trim().is_empty() {
            debug!(
                "Langfuse is disabled/unset; skipping execution export for {}.",
                execution_id
            );
            return Ok(());
        }

        if config.public_key.as_deref().unwrap_or("").trim().is_empty()
            || config.secret_key.as_deref().unwrap_or("").trim().is_empty()
        {
            debug!(
                "Langfuse keys not configured in keyring; skipping export for {}.",
                execution_id
            );
            return Ok(());
        }

        // Locate the execution and its project DB
        let mut target_db = None;
        let mut target_proj_id = None;

        // Check active project DBs first
        for item in state.project_dbs.iter() {
            let pid = *item.key();
            let db = item.value().clone();
            let exists = db
                .with_read_conn(|conn| {
                    Ok(execution_repo::get_execution(conn, execution_id)?.is_some())
                })
                .unwrap_or(false);

            if exists {
                target_db = Some(db);
                target_proj_id = Some(pid);
                break;
            }
        }

        // If not found, iterate through projects in global_db
        if target_db.is_none() {
            let projects = state
                .global_db
                .with_read_conn(project_repo::list_projects)
                .unwrap_or_default();

            for proj in projects {
                if let Some(db) = state.get_project_db(&proj.id) {
                    let exists = db
                        .with_read_conn(|conn| {
                            Ok(execution_repo::get_execution(conn, execution_id)?.is_some())
                        })
                        .unwrap_or(false);

                    if exists {
                        target_db = Some(db);
                        target_proj_id = Some(proj.id);
                        break;
                    }
                }
            }
        }

        let (db, proj_id) = match (target_db, target_proj_id) {
            (Some(db), Some(pid)) => (db, pid),
            _ => {
                debug!(
                    "Execution {} not found in any project database.",
                    execution_id
                );
                return Ok(());
            }
        };

        // Query execution, steps, and token usages from SQLite
        let (execution_opt, steps, token_usages) = db.with_read_conn(|conn| {
            let exec = execution_repo::get_execution(conn, execution_id)?;
            let steps = execution_repo::list_steps(conn, execution_id)?;
            let usages = token_usage_repo::get_token_usages_by_execution(conn, execution_id)?;
            Ok((exec, steps, usages))
        })?;

        let execution = match execution_opt {
            Some(e) => e,
            None => return Ok(()),
        };

        // Assemble IngestionBatch
        let mut batch = IngestionBatch::new();

        // 1. Trace Item
        let trace_id = execution_id.to_string();
        let agent_id_str = execution.agent_instance_id.to_string();
        let trace_name = format!("agent:{}", agent_id_str);
        let pid_str = proj_id.as_str();
        let trace_time = execution.started_at.unwrap_or(execution.updated_at);
        let trace_item = create_trace_item(
            &trace_id,
            &trace_name,
            "trans4mers-sovereign-user",
            &pid_str,
            &pid_str,
            &agent_id_str,
            &format!("{:?}", execution.status),
            &trace_time,
        );
        batch.push(trace_item);

        // 2. Generation Items (Mirrored from persisted token_usages, guaranteeing exact token parity!)
        for usage in &token_usages {
            let prompt_text = sanitize_payload("LLM Generation Input", config.capture_prompts);
            let output_text = sanitize_payload("LLM Generation Output", config.capture_prompts);
            let usage_id_str = usage.id.to_string();

            let gen_item = create_generation_item(
                &usage_id_str,
                &trace_id,
                &usage.model,
                &usage.provider,
                usage.prompt_tokens,
                usage.completion_tokens,
                usage.total_tokens,
                usage.estimated_cost_usd.unwrap_or(0.0),
                usage.compute_time_ms,
                &prompt_text,
                &output_text,
                &usage.created_at,
            );
            batch.push(gen_item);
        }

        // 3. Span Items for Tool Calls and Retrievals
        for step in &steps {
            let start_time = step.created_at;
            let end_time =
                step.created_at + chrono::Duration::milliseconds(step.duration_ms as i64);

            let tool_name = if let Some(ref req) = step.tool_call_request {
                req.get("tool_name")
                    .or_else(|| req.get("tool"))
                    .or_else(|| req.get("name"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "unknown_tool".to_string())
            } else if !step.action_intent.is_empty() {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&step.action_intent) {
                    parsed
                        .get("tool_name")
                        .or_else(|| parsed.get("tool"))
                        .or_else(|| parsed.get("name"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| step.action_intent.clone())
                } else {
                    step.action_intent.clone()
                }
            } else {
                continue;
            };

            let level = "DEFAULT";
            let status_msg: Option<&str> = None;

            let metadata = serde_json::json!({
                "step_number": step.step_number,
                "tool_name": tool_name,
            });

            let input_val = if config.capture_prompts {
                step.tool_call_request.clone()
            } else {
                Some(serde_json::json!("[REDACTED]"))
            };

            let output_val = if config.capture_prompts {
                if step.tool_result_summary.is_empty() {
                    None
                } else {
                    Some(serde_json::Value::String(step.tool_result_summary.clone()))
                }
            } else {
                Some(serde_json::json!("[REDACTED]"))
            };

            let span_id = format!("{}-step-{}", trace_id, step.step_number);
            let span_name = format!("tool:{}", tool_name);

            let span_item = create_span_item(
                &span_id,
                &trace_id,
                &span_name,
                &start_time,
                &end_time,
                level,
                status_msg,
                metadata,
                input_val,
                output_val,
            );
            batch.push(span_item);

            // If this is a retrieval tool (e.g. memory.search or document.search), emit a retrieval span
            if tool_name.contains("search") || tool_name.contains("retrieve") {
                let ret_span_id = format!("{}-retrieval-{}", trace_id, step.step_number);
                let ret_span_name = format!("retrieval:{}", tool_name);
                let ret_item = create_span_item(
                    &ret_span_id,
                    &trace_id,
                    &ret_span_name,
                    &start_time,
                    &end_time,
                    level,
                    status_msg,
                    serde_json::json!({ "retrieval_query": tool_name }),
                    None,
                    None,
                );
                batch.push(ret_item);
            }
        }

        // Send via LangfuseClient
        let client = LangfuseClient::new();
        client.ingest_batch(&batch, &config).await?;

        info!(
            "Successfully mirrored execution {} to Langfuse ({} total items, {} token usages).",
            execution_id,
            batch.len(),
            token_usages.len()
        );

        Ok(())
    }
}
