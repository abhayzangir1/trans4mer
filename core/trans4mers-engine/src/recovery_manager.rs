use rusqlite::{Row, params};
use tracing::{info, warn};
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::ids::{ExecutionId, ProjectId};

pub struct RecoveryManager;

impl RecoveryManager {
    /// Performs true Checkpoint-based Event Replay to recover crashed executions.
    /// Rebuilds the event stream from the last successful checkpoint sequence.
    pub fn recover_crashed_executions(
        global_db: &trans4mers_storage::db_handle::DbHandle,
    ) -> Result<Vec<(ExecutionId, ProjectId)>, Trans4mersError> {
        info!("Running strict crash recovery for interrupted agents via event replay...");

        let projects = global_db.with_read_conn(|conn| {
            let mut stmt = conn
                .prepare("SELECT id, workspace_path FROM projects")
                .map_err(|e| Trans4mersError::Database(e.to_string()))?;
            let iter = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|e| Trans4mersError::Database(e.to_string()))?;

            let mut projs = Vec::new();
            for p in iter {
                projs.push(p.map_err(|e| Trans4mersError::Database(e.to_string()))?);
            }
            Ok(projs)
        })?;

        let mut all_recovered = Vec::new();

        for (proj_id, workspace_path) in projects {
            let db_path = std::path::PathBuf::from(workspace_path)
                .join(".trans4mers")
                .join("project.sqlite");
            if !db_path.exists() {
                continue;
            }

            if let Ok(project_db) = trans4mers_storage::db_handle::DbHandle::open(&db_path) {
                let pid = match ProjectId::from_str(&proj_id) {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                let recovered_ids: Vec<String> = project_db.with_exclusive_conn(|conn| {
                    let tx = conn.transaction().map_err(|e| Trans4mersError::Database(e.to_string()))?;

                    let mut crashed_ids = Vec::new();
                    {
                        let mut stmt = tx.prepare(
                            "SELECT id FROM agent_executions WHERE status IN ('Running', 'Checkpointing')"
                        ).map_err(|e| Trans4mersError::Database(e.to_string()))?;

                        let rows = stmt.query_map([], |row| row.get::<_, String>(0))
                            .map_err(|e| Trans4mersError::Database(e.to_string()))?;

                        for id in rows.flatten() {
                            crashed_ids.push(id);
                        }
                    }

                    for exec_id_str in &crashed_ids {
                        // Find the last checkpoint sequence id and context snapshot
                        let cp_data: Option<(i64, Option<String>)> = tx.query_row(
                            "SELECT COALESCE(last_event_sequence, 0), context_snapshot FROM execution_checkpoints WHERE execution_id = ?1",
                            params![exec_id_str],
                            |row| Ok((row.get(0)?, row.get(1)?))
                        ).ok();

                        let (cp_seq, context_json) = cp_data.unwrap_or((0, None));

                        // True Event Replay logic: fetch all events that occurred after the checkpoint.
                        let mut event_stmt = tx.prepare(
                            "SELECT sequence_id, payload FROM domain_events WHERE sequence_id > ?1 ORDER BY sequence_id ASC"
                        ).map_err(|e| Trans4mersError::Database(e.to_string()))?;

                        let unprojected_events: Vec<(i64, String)> = match event_stmt.query_map(params![cp_seq], |row: &Row| {
                            Ok((row.get(0)?, row.get(1)?))
                        }) {
                            Ok(ev_rows) => ev_rows.filter_map(Result::ok).collect(),
                            Err(e) => {
                                tracing::warn!("Failed to query unprojected events for execution {}: {}", exec_id_str, e);
                                Vec::new()
                            }
                        };

                        let mut highest_seq = cp_seq;
                        let mut recovered_generation: u64 = tx.query_row(
                            "SELECT COALESCE(generation, 0) FROM execution_checkpoints WHERE execution_id = ?1",
                            params![exec_id_str],
                            |row| row.get(0),
                        ).unwrap_or(0);

                        if let Some(snapshot) = context_json
                            && let Ok(mut state) = serde_json::from_str::<trans4mers_domain::execution::ExecutionState>(&snapshot) {
                                for (seq, ev_json) in &unprojected_events {
                                    if *seq > highest_seq {
                                        highest_seq = *seq;
                                    }
                                      if let Ok(ev) = serde_json::from_str::<trans4mers_domain::event::DomainEvent>(ev_json) {
                                          match ev {
                                              trans4mers_domain::event::DomainEvent::ExecutionStepCompleted { execution_id, step_number } => {
                                                  if execution_id.as_str() == *exec_id_str && step_number > state.total_steps_executed {
                                                      state.total_steps_executed = step_number;
                                                  }
                                              }
                                              trans4mers_domain::event::DomainEvent::ToolExecuted { execution_id, tool_name, success } => {
                                                  if execution_id.as_str() == *exec_id_str {
                                                      state.total_steps_executed += 1;
                                                      state.steps.push(trans4mers_domain::execution::ReActStep {
                                                          step_index: state.total_steps_executed,
                                                          thought: format!("Replayed tool execution: {}", tool_name),
                                                          action_intent: Some(serde_json::json!({ "tool_name": tool_name })),
                                                          result_payload: Some(serde_json::json!({ "success": success })),
                                                          error: None,
                                                          token_usage: None,
                                                          created_at: chrono::Utc::now(),
                                                          completed_at: Some(chrono::Utc::now()),
                                                      });
                                                  }
                                              }
                                              trans4mers_domain::event::DomainEvent::MessageSent { message } => {
                                                  if message.conversation_id == state.conversation_id {
                                                      state.total_steps_executed += 1;
                                                      state.steps.push(trans4mers_domain::execution::ReActStep {
                                                          step_index: state.total_steps_executed,
                                                          thought: "Replayed message sent".to_string(),
                                                          action_intent: Some(serde_json::json!({ "type": "MessageSent" })),
                                                          result_payload: Some(serde_json::json!({ "message_id": message.id })),
                                                          error: None,
                                                          token_usage: None,
                                                          created_at: chrono::Utc::now(),
                                                          completed_at: Some(chrono::Utc::now()),
                                                      });
                                                  }
                                              }
                                              trans4mers_domain::event::DomainEvent::PendingApproval { execution_id, approval_id, tool_name, .. } => {
                                                  if execution_id.as_str() == *exec_id_str {
                                                      state.total_steps_executed += 1;
                                                      state.steps.push(trans4mers_domain::execution::ReActStep {
                                                          step_index: state.total_steps_executed,
                                                          thought: format!("Replayed pending approval for tool: {}", tool_name),
                                                          action_intent: Some(serde_json::json!({ "type": "PendingApproval", "approval_id": approval_id })),
                                                          result_payload: None,
                                                          error: None,
                                                          token_usage: None,
                                                          created_at: chrono::Utc::now(),
                                                          completed_at: Some(chrono::Utc::now()),
                                                      });
                                                  }
                                              }
                                              trans4mers_domain::event::DomainEvent::MemoryStored { memory_id, .. } => {
                                                  state.total_steps_executed += 1;
                                                  state.steps.push(trans4mers_domain::execution::ReActStep {
                                                      step_index: state.total_steps_executed,
                                                      thought: "Replayed memory stored".to_string(),
                                                      action_intent: Some(serde_json::json!({ "type": "MemoryStored", "memory_id": memory_id })),
                                                      result_payload: None,
                                                      error: None,
                                                      token_usage: None,
                                                      created_at: chrono::Utc::now(),
                                                      completed_at: Some(chrono::Utc::now()),
                                                  });
                                              }
                                              _ => {}
                                          }
                                      }
                                 }
                                 recovered_generation = state.total_steps_executed as u64;
                                 if let Ok(updated_snapshot) = serde_json::to_string(&state) {
                                     let _ = tx.execute(
                                         "UPDATE execution_checkpoints SET context_snapshot = ?1, last_event_sequence = ?2, step_number = ?3, generation = ?4 WHERE execution_id = ?5",
                                         params![updated_snapshot, highest_seq, state.total_steps_executed, state.total_steps_executed as u64, exec_id_str],
                                     );
                                 }
                             }

                         tx.execute(
                             "UPDATE agent_executions SET status = 'Queued', generation = ?1, updated_at = ?2 WHERE id = ?3",
                             params![recovered_generation, chrono::Utc::now().to_rfc3339(), exec_id_str]
                         ).map_err(|e| Trans4mersError::Database(e.to_string()))?;

                        info!("[{}] Recovered Execution {}: replayed {} uncheckpointed events into memory context.", proj_id, exec_id_str, unprojected_events.len());
                    }

                    let reverted_messages = tx.execute(
                        "UPDATE inbox_messages SET delivery_state = 'QUEUED', claimed_at = NULL WHERE delivery_state = 'CLAIMED'",
                        []
                    ).map_err(|e| Trans4mersError::Database(e.to_string()))?;

                    if reverted_messages > 0 {
                        warn!("[{}] Reverted {} claimed messages to queued state.", proj_id, reverted_messages);
                    }

                    tx.commit().map_err(|e| Trans4mersError::Database(e.to_string()))?;

                    Ok(crashed_ids)
                }).unwrap_or_default();

                for eid_str in recovered_ids {
                    if let Ok(eid) = ExecutionId::from_str(&eid_str) {
                        all_recovered.push((eid, pid));
                    }
                }
            }
        }

        Ok(all_recovered)
    }
}
