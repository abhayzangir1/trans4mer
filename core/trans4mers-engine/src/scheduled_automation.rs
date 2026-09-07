use crate::app_state::AppState;
use chrono::{DateTime, Utc};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::ids::{AgentInstanceId, ConversationId, ProjectId};
use trans4mers_storage::repos::scheduled_task_repo::{self, ScheduledTask};

pub struct ScheduledAutomationManager;

impl ScheduledAutomationManager {
    /// Spawns the background heartbeat that periodically checks for and fires due scheduled tasks.
    pub fn spawn_heartbeat_loop(app_state: Arc<AppState>) {
        tokio::spawn(async move {
            info!("Scheduled automation heartbeat started. Ticking every 10 seconds.");
            let mut ticker = tokio::time::interval(Duration::from_secs(10));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                ticker.tick().await;

                if app_state.shutdown_manager.is_shutting_down() {
                    debug!("Shutdown received in scheduled automation loop. Exiting.");
                    break;
                }

                let now = Utc::now();
                for entry in app_state.project_dbs.iter() {
                    let pdb = entry.value().clone();
                    let project_id = *entry.key();

                    let due_tasks = match pdb
                        .with_read_conn(|conn| scheduled_task_repo::list_due_tasks(conn, now))
                    {
                        Ok(tasks) => tasks,
                        Err(e) => {
                            warn!(
                                "Error listing due scheduled tasks for project {}: {}",
                                project_id, e
                            );
                            continue;
                        }
                    };

                    for task in due_tasks {
                        info!(
                            "Firing scheduled task '{}' ({}) for Agent {}",
                            task.human_readable, task.id, task.target_agent_id
                        );

                        // 1. Resolve agent instance id
                        let agent_id = match AgentInstanceId::from_str(&task.target_agent_id) {
                            Ok(id) => id,
                            Err(e) => {
                                error!(
                                    "Invalid agent_id in scheduled task {}: {} ({}). Advancing next run.",
                                    task.id, task.target_agent_id, e
                                );
                                let next_run = Self::compute_next_run(&task.cron_expression, now);
                                let _ = pdb.with_write_tx(|conn| {
                                    scheduled_task_repo::update_task_run(
                                        conn, &task.id, now, next_run,
                                    )
                                });
                                continue;
                            }
                        };

                        // 2. Resolve or create execution id
                        let mut exec_id_opt: Option<trans4mers_domain::ids::ExecutionId> = None;
                        let _ = pdb.with_read_conn(|conn| {
                            if let Ok(exec_id_str) = conn.query_row(
                                "SELECT id FROM agent_executions WHERE agent_instance_id = ?1 AND status IN ('Pending', 'Running', 'Checkpointing', 'WaitingForApproval', 'WaitingForMessage')",
                                rusqlite::params![agent_id.as_str()],
                                |row| row.get::<_, String>(0)
                            ) {
                                exec_id_opt = trans4mers_domain::ids::ExecutionId::from_str(&exec_id_str).ok();
                            }
                            Ok(())
                        });

                        let exec_id = if let Some(id) = exec_id_opt {
                            id
                        } else {
                            let new_id = trans4mers_domain::ids::ExecutionId::new();
                            let _ = pdb.with_write_tx(|conn| {
                                conn.execute(
                                    "INSERT INTO agent_executions (id, agent_instance_id, conversation_id, status, generation, current_step, max_steps, updated_at) VALUES (?1, ?2, ?3, 'Pending', 0, 0, 100, ?4)",
                                    rusqlite::params![new_id.as_str(), agent_id.as_str(), task.conversation_id, chrono::Utc::now().to_rfc3339()]
                                )?;
                                Ok(())
                            });
                            new_id
                        };

                        // 3. Inject message payload into durable inbox via CQRS
                        let payload_str = format!(
                            "[Scheduled Task Trigger - {}]: {}",
                            task.human_readable, task.action_prompt
                        );
                        let inbox_event =
                            trans4mers_domain::event::DomainEvent::InboxMessageQueued {
                                message_id: uuid::Uuid::new_v4().to_string(),
                                recipient_agent_id: agent_id,
                                sender_actor_id: "scheduler".to_string(),
                                payload: payload_str,
                            };

                        match pdb.with_write_tx(|conn| {
                            crate::cqrs::commit_event(
                                conn,
                                inbox_event,
                                trans4mers_domain::ids::ActorId::new(),
                            )
                        }) {
                            Ok(envelope) => {
                                let env_arc = std::sync::Arc::new(envelope);
                                let bus = app_state.get_event_bus(&project_id);
                                let _ = bus.publish(env_arc.clone());
                                let _ = app_state.global_event_bus.publish(env_arc);
                                app_state.scheduler.queue(exec_id, project_id);
                            }
                            Err(e) => {
                                error!(
                                    "Failed to commit inbox event for scheduled task {}: {}",
                                    task.id, e
                                );
                            }
                        }

                        // 4. Compute next run timestamp and update task record
                        let next_run = Self::compute_next_run(&task.cron_expression, now);
                        if let Err(e) = pdb.with_write_tx(|conn| {
                            scheduled_task_repo::update_task_run(conn, &task.id, now, next_run)
                        }) {
                            error!(
                                "Failed to advance scheduled task run for {}: {}",
                                task.id, e
                            );
                        }
                    }
                }
            }
        });
    }

    /// Helper to parse human-readable interval strings into approximate next DateTime
    pub fn compute_next_run(expression: &str, from_time: DateTime<Utc>) -> Option<DateTime<Utc>> {
        let clean = expression.trim().to_lowercase();

        if clean.starts_with("every ") {
            let rest = clean.trim_start_matches("every ").trim();
            if let Some(secs) = Self::parse_duration_string(rest) {
                return Some(from_time + chrono::Duration::seconds(secs as i64));
            }
        }

        // Standard fallback for minute intervals e.g. "*/5 * * * *"
        if clean.starts_with("*/")
            && let Some(first_part) = clean.split_whitespace().next()
            && let Ok(num) = first_part.trim_start_matches("*/").parse::<i64>()
        {
            return Some(from_time + chrono::Duration::minutes(num.max(1)));
        }

        // Default to 1 hour if unknown
        Some(from_time + chrono::Duration::hours(1))
    }

    fn parse_duration_string(s: &str) -> Option<u64> {
        let s = s.trim();
        if s.ends_with("s") || s.ends_with("sec") || s.ends_with("seconds") {
            let num = s.chars().take_while(|c| c.is_numeric()).collect::<String>();
            num.parse::<u64>().ok()
        } else if s.ends_with("m") || s.ends_with("min") || s.ends_with("minutes") {
            let num = s.chars().take_while(|c| c.is_numeric()).collect::<String>();
            num.parse::<u64>().ok().map(|m| m * 60)
        } else if s.ends_with("h") || s.ends_with("hr") || s.ends_with("hours") {
            let num = s.chars().take_while(|c| c.is_numeric()).collect::<String>();
            num.parse::<u64>().ok().map(|h| h * 3600)
        } else if s.ends_with("d") || s.ends_with("day") || s.ends_with("days") {
            let num = s.chars().take_while(|c| c.is_numeric()).collect::<String>();
            num.parse::<u64>().ok().map(|d| d * 86400)
        } else {
            None
        }
    }

    /// Creates and persists a new scheduled task
    pub fn schedule_task(
        app_state: &AppState,
        project_id: &ProjectId,
        conversation_id: &ConversationId,
        target_agent_id: &AgentInstanceId,
        cron_or_interval: &str,
        action_prompt: &str,
    ) -> Result<ScheduledTask, Trans4mersError> {
        let pdb = app_state.get_project_db(project_id).ok_or_else(|| {
            Trans4mersError::Internal(format!("Project DB not found: {}", project_id))
        })?;

        let now = Utc::now();
        let next_run = Self::compute_next_run(cron_or_interval, now);

        let task = ScheduledTask {
            id: format!("sched_{}", uuid::Uuid::new_v4().simple()),
            project_id: project_id.to_string(),
            conversation_id: conversation_id.to_string(),
            target_agent_id: target_agent_id.to_string(),
            cron_expression: cron_or_interval.to_string(),
            human_readable: cron_or_interval.to_string(),
            action_prompt: action_prompt.to_string(),
            is_active: true,
            last_run_at: None,
            next_run_at: next_run,
            created_at: now,
        };

        pdb.with_write_tx(|conn| scheduled_task_repo::create_task(conn, &task))?;

        info!(
            "Created scheduled task {} with next run at {:?}",
            task.id, task.next_run_at
        );
        Ok(task)
    }
}
