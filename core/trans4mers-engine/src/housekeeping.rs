use crate::app_state::AppState;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};
use trans4mers_domain::ids::ProjectId;
use trans4mers_storage::repos::lock_repo::LockRepo;
use trans4mers_storage::repos::project_repo;

pub struct Housekeeper;

impl Housekeeper {
    /// Spawns a background task that executes every 60 seconds until shutdown is signaled.
    pub fn spawn(app_state: Arc<AppState>) {
        tokio::spawn(async move {
            info!("Starting background housekeeping loop (60s interval)...");
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            // First tick completes immediately, skip it to let system warm up
            interval.tick().await;

            loop {
                interval.tick().await;

                if app_state.shutdown_manager.is_shutting_down() {
                    info!("Housekeeper received shutdown signal. Terminating loop.");
                    break;
                }

                Self::run_housekeeping_cycle(&app_state).await;
            }
        });
    }

    /// Runs a single iteration of system housekeeping across all active projects.
    pub async fn run_housekeeping_cycle(app_state: &AppState) {
        // 1. Fetch all known project IDs from global DB
        let mut project_ids: Vec<ProjectId> = Vec::new();
        let _ = app_state.global_db.with_read_conn(|conn| {
            if let Ok(projects) = project_repo::list_projects(conn) {
                for p in projects {
                    project_ids.push(p.id);
                }
            }
            Ok(())
        });

        for proj_id in project_ids {
            if let Some(db) = app_state.get_project_db(&proj_id) {
                // a. Purge expired advisory locks
                let lock_repo = LockRepo::new(db.clone());
                if let Ok(purged) = lock_repo.purge_expired()
                    && purged > 0
                {
                    info!(project_id = %proj_id, count = purged, "Purged expired advisory locks");
                }

                // b. Prune checkpoints older than 24 hours
                let _ = db.with_write_tx(|conn| {
                    if let Ok(pruned) = crate::checkpoint_manager::CheckpointManager::prune_old_checkpoints(conn, 24)
                        && pruned > 0 {
                            info!(project_id = %proj_id, count = pruned, "Pruned stale checkpoints older than 24h");
                        }
                    Ok(())
                });

                // c. Detect orphaned/stalled executions (status Running/Checkpointing and no update in > 10 min)
                let cutoff = (chrono::Utc::now() - chrono::Duration::minutes(10)).to_rfc3339();
                let mut stalled_ids: Vec<String> = Vec::new();
                let _ = db.with_read_conn(|conn| {
                    let mut stmt = conn.prepare(
                        "SELECT id FROM agent_executions WHERE status IN ('Running', 'Checkpointing') AND updated_at < ?1"
                    ).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                    let rows = stmt.query_map([&cutoff], |row| row.get::<_, String>(0))
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                    for r in rows.flatten() {
                        stalled_ids.push(r);
                    }
                    Ok(())
                });

                if !stalled_ids.is_empty() {
                    let envelopes: Vec<trans4mers_domain::event::EventEnvelope> = db.with_write_tx(|conn| {
                        let mut envs = Vec::new();
                        for exec_id_str in &stalled_ids {
                            warn!(execution_id = %exec_id_str, "Marking stalled/orphaned execution as Failed");
                            let _ = conn.execute(
                                "UPDATE agent_executions SET status = 'Failed', updated_at = ?2 WHERE id = ?1",
                                rusqlite::params![exec_id_str, chrono::Utc::now().to_rfc3339()]
                            );
                            if let Ok(exec_id) = trans4mers_domain::ids::ExecutionId::from_str(exec_id_str) {
                                let ev = trans4mers_domain::event::DomainEvent::ExecutionFailed {
                                    execution_id: exec_id,
                                    reason: "Orphaned execution timed out without heartbeat in 10 minutes".to_string(),
                                };
                                if let Ok(envelope) = crate::cqrs::commit_event(conn, ev, trans4mers_domain::ids::ActorId::new()) {
                                    envs.push(envelope);
                                }
                            }
                        }
                        Ok(envs)
                    }).unwrap_or_default();

                    let bus = app_state.get_event_bus(&proj_id);
                    for envelope in envelopes {
                        let env_arc = std::sync::Arc::new(envelope);
                        let _ = bus.publish(env_arc.clone());
                        let _ = app_state.global_event_bus.publish(env_arc);
                    }
                }

                // d. Run 4-tier memory promotion and pruning
                let _ = crate::memory_tier_promoter::MemoryTierPromoter::run_promotion_cycle(
                    app_state, &proj_id,
                )
                .await;

                // e. Run conversation cognitive distillation
                let _ =
                    crate::distillation_engine::DistillationEngine::run_cycle(app_state, &proj_id)
                        .await;

                // f. Invalidation & expiry sweep for memories
                let _ = db.with_write_tx(|conn| {
                    let now_str = chrono::Utc::now().to_rfc3339();
                    let purged = conn.execute(
                        "DELETE FROM project_memories WHERE expires_at IS NOT NULL AND expires_at <= ?1",
                        rusqlite::params![now_str],
                    ).unwrap_or(0);
                    if purged > 0 {
                        tracing::info!(project_id = %proj_id, count = purged, "Purged expired ephemeral memories");
                    }
                    Ok(())
                });
            }
        }
    }
}
