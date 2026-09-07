use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::{Notify, OwnedSemaphorePermit, Semaphore, mpsc};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::ids::{ExecutionId, ProjectId};

pub struct QueueEntry {
    pub execution_id: ExecutionId,
    pub project_id: ProjectId,
    pub priority: u32,
}

pub struct WakeRequest {
    pub execution_id: ExecutionId,
    pub priority_boost: bool,
}

/// Controls the maximum number of concurrent agents executing the ReAct loop
/// with wake latching, duplicate execution prevention, and project concurrency caps.
pub struct Scheduler {
    permits: Arc<Semaphore>,
    new_queue_tx: mpsc::Sender<QueueEntry>,
    wake_queue_tx: mpsc::Sender<WakeRequest>,
    cancellation_tokens: Arc<DashMap<ExecutionId, CancellationToken>>,
    wake_latch: Arc<DashMap<ExecutionId, bool>>,
    running_executions: Arc<DashMap<ExecutionId, ProjectId>>,
    project_caps: Arc<DashMap<ProjectId, usize>>,
    project_active: Arc<DashMap<ProjectId, usize>>,
    slot_available: Arc<Notify>,
}

impl Scheduler {
    pub fn new(
        max_concurrent_agents: usize,
    ) -> (
        Self,
        mpsc::Receiver<QueueEntry>,
        mpsc::Receiver<WakeRequest>,
    ) {
        let (new_tx, new_rx) = mpsc::channel(256);
        let (wake_tx, wake_rx) = mpsc::channel(256);

        let scheduler = Self {
            permits: Arc::new(Semaphore::new(max_concurrent_agents)),
            new_queue_tx: new_tx,
            wake_queue_tx: wake_tx,
            cancellation_tokens: Arc::new(DashMap::new()),
            wake_latch: Arc::new(DashMap::new()),
            running_executions: Arc::new(DashMap::new()),
            project_caps: Arc::new(DashMap::new()),
            project_active: Arc::new(DashMap::new()),
            slot_available: Arc::new(Notify::new()),
        };

        (scheduler, new_rx, wake_rx)
    }

    /// Safely releases an active project execution slot and notifies waiting tasks
    pub fn release_project_slot(&self, project_id: &ProjectId) {
        self.project_active
            .entry(*project_id)
            .and_modify(|v| *v = v.saturating_sub(1));
        self.slot_available.notify_waiters();
    }

    /// Actually boots the orchestrator daemon. Called from main.rs.
    pub fn start_background_loop(
        &self,
        app_state: Arc<crate::app_state::AppState>,
        mut new_rx: mpsc::Receiver<QueueEntry>,
        mut wake_rx: mpsc::Receiver<WakeRequest>,
    ) {
        let scheduler = app_state.scheduler.clone();
        let permits = self.permits.clone();
        let mut shutdown_rx = app_state.shutdown_manager.subscribe();
        let tokens = self.cancellation_tokens.clone();

        tokio::spawn(async move {
            info!("Scheduler background orchestrator started. Listening for executions.");
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        info!("Scheduler received shutdown signal. Cancelling all running agent tasks...");
                        for entry in tokens.iter() {
                            entry.value().cancel();
                        }
                        info!("Terminating orchestrator loop.");
                        break;
                    }
                    Some(entry) = new_rx.recv() => {
                        // Deduplication: if already running, latch a wake instead of spawning duplicate
                        if scheduler.running_executions.contains_key(&entry.execution_id) {
                            debug!("Execution {} is already running; latching wake instead of double-scheduling.", entry.execution_id);
                            scheduler.wake_latch.insert(entry.execution_id, true);
                            continue;
                        }

                        Self::try_schedule(app_state.clone(), entry, permits.clone()).await;
                    }
                    Some(wake) = wake_rx.recv() => {
                        // If actively running, latch it so it immediately re-schedules on step finish
                        if scheduler.running_executions.contains_key(&wake.execution_id) {
                            debug!("Execution {} actively running. Latched wake request.", wake.execution_id);
                            scheduler.wake_latch.insert(wake.execution_id, true);
                        } else {
                            // Find project_id for this execution and queue it
                            let mut found_project = None;
                            for p_entry in app_state.project_dbs.iter() {
                                let pid = p_entry.key();
                                if let Ok(Some(_)) = p_entry.value().with_read_conn(|conn| {
                                    trans4mers_storage::repos::execution_repo::get_execution(conn, &wake.execution_id)
                                }) {
                                    found_project = Some(*pid);
                                    break;
                                }
                            }

                            if let Some(pid) = found_project {
                                scheduler.queue(wake.execution_id, pid);
                            } else {
                                debug!("Could not find project for execution {}. Wake ignored.", wake.execution_id);
                            }
                        }
                    }
                }
            }
        });
    }

    /// Queue a new execution for scheduling.
    pub fn queue(&self, execution_id: ExecutionId, project_id: ProjectId) {
        let _ = self.new_queue_tx.try_send(QueueEntry {
            execution_id,
            project_id,
            priority: 0,
        });
    }

    /// Re-queue a resumed execution with wake latching semantics
    pub fn wake(&self, execution_id: ExecutionId, priority_boost: bool) {
        let _ = self.wake_queue_tx.try_send(WakeRequest {
            execution_id,
            priority_boost,
        });
    }

    async fn try_schedule(
        app_state: Arc<crate::app_state::AppState>,
        entry: QueueEntry,
        permits: Arc<Semaphore>,
    ) {
        tokio::spawn(async move {
            let scheduler = app_state.scheduler.clone();
            let mut shutdown_rx = app_state.shutdown_manager.subscribe();

            // 1. Strictly and atomically enforce project concurrency cap BEFORE acquiring global permit.
            // This prevents a saturated project from hoarding global permits while waiting for its own project slot,
            // which would starve other projects and deadlock the scheduler.
            loop {
                let cap = scheduler
                    .project_caps
                    .get(&entry.project_id)
                    .map(|c| *c.value())
                    .unwrap_or(4);
                let mut current_entry = scheduler
                    .project_active
                    .entry(entry.project_id)
                    .or_insert(0);
                if *current_entry < cap {
                    *current_entry += 1;
                    break;
                }
                drop(current_entry);
                debug!(
                    "Project {} at concurrency cap. Waiting for active slot...",
                    entry.project_id
                );
                let notified = scheduler.slot_available.notified();
                tokio::select! {
                    _ = shutdown_rx.recv() => return,
                    _ = notified => {}
                }
            }

            // 2. Now acquire global node execution permit
            let permit = match permits.acquire_owned().await {
                Ok(p) => p,
                Err(e) => {
                    error!("Failed to acquire scheduling permit: {}", e);
                    scheduler.release_project_slot(&entry.project_id);
                    return;
                }
            };

            // Register running state
            scheduler
                .running_executions
                .insert(entry.execution_id, entry.project_id);

            info!(
                "Acquired permit for Execution {}, booting agent loop...",
                entry.execution_id
            );

            let db = match app_state.get_project_db(&entry.project_id) {
                Some(db) => db,
                None => {
                    error!("Project DB not found for execution {}", entry.execution_id);
                    scheduler.running_executions.remove(&entry.execution_id);
                    scheduler.release_project_slot(&entry.project_id);
                    return;
                }
            };

            // 1. Fetch Execution metadata
            let (agent_id, generation, _status, convo_id): (
                trans4mers_domain::ids::AgentInstanceId,
                u64,
                String,
                trans4mers_domain::ids::ConversationId,
            ) = match db.with_read_conn(|conn| {
                let row = conn.query_row(
                    "SELECT agent_instance_id, generation, status, conversation_id FROM agent_executions WHERE id = ?1",
                    rusqlite::params![entry.execution_id.as_str()],
                    |row| {
                        let a: String = row.get(0)?;
                        let g: i64 = row.get(1)?;
                        let s: String = row.get(2)?;
                        let c: String = row.get(3)?;
                        Ok((a, g, s, c))
                    }
                )?;
                Ok(row)
            }) {
                Ok(res) => (
                    std::str::FromStr::from_str(&res.0).unwrap_or_default(),
                    res.1 as u64,
                    res.2,
                    std::str::FromStr::from_str(&res.3).unwrap_or_else(|_| trans4mers_domain::ids::ConversationId::new())
                ),
                Err(e) => {
                    error!("Failed to fetch execution {}: {}", entry.execution_id, e);
                    scheduler.running_executions.remove(&entry.execution_id);
                    scheduler.release_project_slot(&entry.project_id);
                    return;
                }
            };

            // 2. Load Checkpoint or Create New
            let state = match db.with_read_conn(|conn| {
                crate::checkpoint_manager::CheckpointManager::load_checkpoint(
                    conn,
                    &entry.execution_id,
                    generation,
                )
            }) {
                Ok(Some(s)) => s,
                Ok(None) => trans4mers_domain::execution::ExecutionState {
                    execution_id: entry.execution_id,
                    conversation_id: convo_id,
                    steps: vec![],
                    total_steps_executed: 0,
                },
                Err(e) => {
                    error!(
                        "Failed to load checkpoint for execution {}: {}",
                        entry.execution_id, e
                    );
                    scheduler.running_executions.remove(&entry.execution_id);
                    scheduler.release_project_slot(&entry.project_id);
                    return;
                }
            };

            // 3. Fetch Agent Instance
            let (def_id, _caps): (trans4mers_domain::ids::AgentDefinitionId, String) = match db
                .with_read_conn(|conn| {
                    let row = conn.query_row(
                        "SELECT definition_id, capabilities FROM agent_instances WHERE id = ?1",
                        rusqlite::params![agent_id.as_str()],
                        |row| {
                            let d: String = row.get(0)?;
                            let c: String = row.get(1)?;
                            Ok((d, c))
                        },
                    )?;
                    Ok(row)
                }) {
                Ok(res) => (
                    std::str::FromStr::from_str(&res.0).unwrap_or_else(|_| {
                        trans4mers_domain::ids::AgentDefinitionId::from_str("boss")
                            .unwrap_or_else(|_| trans4mers_domain::ids::AgentDefinitionId::new())
                    }),
                    res.1,
                ),
                Err(e) => {
                    error!("Agent instance not found {}: {}", agent_id, e);
                    scheduler.running_executions.remove(&entry.execution_id);
                    scheduler.release_project_slot(&entry.project_id);
                    return;
                }
            };

            // 4. Fetch Agent Definition from Global DB
            let def_id_str = def_id.to_string();
            let (instructions, model_config) = match app_state.global_db.with_read_conn(|conn| {
                let row = conn.query_row(
                    "SELECT system_instructions, default_model_config FROM agent_definitions WHERE id = ?1",
                    rusqlite::params![def_id_str],
                    |row| {
                        let i: String = row.get(0)?;
                        let m: String = row.get(1)?;
                        Ok((i, m))
                    }
                )?;
                Ok(row)
            }) {
                Ok(res) => {
                    let m: trans4mers_domain::config::ModelConfig = serde_json::from_str(&res.1).unwrap_or_default();
                    (res.0, m)
                },
                Err(e) => {
                    error!("Agent definition not found {}: {}", def_id, e);
                    scheduler.running_executions.remove(&entry.execution_id);
                    scheduler.release_project_slot(&entry.project_id);
                    return;
                }
            };

            // 5. Get Provider
            let provider = match app_state.provider_registry.get(&model_config.provider) {
                Ok(p) => p,
                Err(e) => {
                    error!(
                        "Provider not found for execution {}: {}",
                        entry.execution_id, e
                    );
                    scheduler.running_executions.remove(&entry.execution_id);
                    scheduler.release_project_slot(&entry.project_id);
                    return;
                }
            };

            let cancel_token = CancellationToken::new();
            app_state
                .scheduler
                .register_cancellation(entry.execution_id, cancel_token.clone());

            let mut workspace_path = None;
            let _ = app_state.global_db.with_read_conn(|conn| {
                if let Ok(Some(proj)) =
                    trans4mers_storage::repos::project_repo::get_project(conn, &entry.project_id)
                {
                    workspace_path = Some(std::path::PathBuf::from(proj.workspace_path));
                }
                Ok(())
            });

            let memory_engine = crate::memory_engine::MemoryEngine::new(app_state.clone());

            // 6. Spawn the real ReAct loop
            let res = crate::agent_runtime::run_agent_execution(
                state,
                provider,
                model_config,
                permit,
                &instructions,
                Some(&memory_engine),
                &agent_id,
                &entry.project_id,
                workspace_path,
                app_state.clone(),
                cancel_token,
            )
            .await;

            // Clean up running execution tracking and cancellation token
            scheduler.running_executions.remove(&entry.execution_id);
            scheduler.cancellation_tokens.remove(&entry.execution_id);
            scheduler.release_project_slot(&entry.project_id);

            match res {
                Ok(_) => info!("Execution {} completed successfully.", entry.execution_id),
                Err(e) => error!("Execution {} failed: {}", entry.execution_id, e),
            }

            // Check wake latch: if wake arrived while execution was running, re-queue now
            if scheduler.wake_latch.remove(&entry.execution_id).is_some() {
                debug!(
                    "Execution {} has latched wake request; re-queueing immediately.",
                    entry.execution_id
                );
                scheduler.queue(entry.execution_id, entry.project_id);
            }
        });
    }

    /// Acquires a permit to execute. Blocks if max concurrency is reached.
    pub async fn acquire_permit(&self) -> Result<OwnedSemaphorePermit, Trans4mersError> {
        self.permits
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| Trans4mersError::Internal(format!("Semaphore closed: {}", e)))
    }

    pub fn try_acquire(&self) -> Option<tokio::sync::OwnedSemaphorePermit> {
        self.permits.clone().try_acquire_owned().ok()
    }

    pub fn register_cancellation(&self, execution_id: ExecutionId, token: CancellationToken) {
        self.cancellation_tokens.insert(execution_id, token);
    }

    /// Hard-cancels an execution loop by triggering its CancellationToken
    pub fn cancel_execution(
        &self,
        execution_id: &trans4mers_domain::ids::ExecutionId,
        _app_state: &crate::app_state::AppState,
    ) {
        if let Some(token) = self.cancellation_tokens.get(execution_id) {
            token.cancel();
        }
    }

    pub fn available_permits(&self) -> usize {
        self.permits.available_permits()
    }
}
