use crate::app_state::AppState;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;
use trans4mers_domain::config::ModelConfig;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::event::DomainEvent;
use trans4mers_domain::ids::{ActorId, AgentInstanceId, ConversationId, ProjectId};
use trans4mers_domain::message::{Message, MessageKind};
use trans4mers_domain::provider::{LlmMessage, LlmRequest};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SwarmPattern {
    Supervisor {
        supervisor_id: AgentInstanceId,
        worker_ids: Vec<AgentInstanceId>,
    },
    FanOut {
        worker_ids: Vec<AgentInstanceId>,
    },
    Debate {
        proponent_id: AgentInstanceId,
        opponent_id: AgentInstanceId,
        rounds: usize,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmResult {
    pub pattern: String,
    pub final_output: String,
    pub transcript: Vec<(String, String)>, // (Actor, Message)
}

pub struct SwarmOrchestrator;

impl SwarmOrchestrator {
    /// Executes a multi-agent debate where two agents challenge and refine each other's reasoning
    /// through adversarial generation rounds using the active LLM provider.
    pub async fn run_debate(
        app_state: Arc<AppState>,
        project_id: ProjectId,
        conversation_id: ConversationId,
        topic: String,
        proponent_id: AgentInstanceId,
        opponent_id: AgentInstanceId,
        rounds: usize,
    ) -> Result<SwarmResult, Trans4mersError> {
        let rounds = rounds.clamp(1, 10);
        info!(topic = %topic, rounds = rounds, "Starting multi-agent debate session");

        let db = app_state
            .get_project_db(&project_id)
            .ok_or_else(|| Trans4mersError::Database("Project DB not found".to_string()))?;

        db.with_read_conn(|conn| {
            let prop_exists: bool = conn
                .query_row(
                    "SELECT 1 FROM agent_instances WHERE id = ?1",
                    rusqlite::params![proponent_id.as_str()],
                    |_| Ok(true),
                )
                .unwrap_or(false);
            let opp_exists: bool = conn
                .query_row(
                    "SELECT 1 FROM agent_instances WHERE id = ?1",
                    rusqlite::params![opponent_id.as_str()],
                    |_| Ok(true),
                )
                .unwrap_or(false);
            if !prop_exists || !opp_exists {
                return Err(Trans4mersError::AgentNotFound(
                    "Proponent or opponent agent instance not found in project database"
                        .to_string(),
                ));
            }
            Ok(())
        })?;

        let _permit = app_state.scheduler.acquire_permit().await?;

        let provider = app_state.provider_registry.get_default().map_err(|e| {
            Trans4mersError::Internal(format!(
                "No LLM provider available for multi-agent debate: {}",
                e
            ))
        })?;
        let mut transcript: Vec<(String, String)> = Vec::new();
        let mut last_critic_critique = String::new();

        for r in 1..=rounds {
            // 1. Proponent Turn
            let prop_prompt = if r == 1 {
                format!(
                    "You are the Proponent in a formal debate on: '{}'. Present your core affirmative argument with supporting evidence.",
                    topic
                )
            } else {
                format!(
                    "Debate Round {}/{} on '{}'. The Critic argued:\n\"{}\"\nDefend your proposition, rebut the critique, and refine your stance.",
                    r, rounds, topic, last_critic_critique
                )
            };

            let cfg = ModelConfig {
                provider: provider.name().to_string(),
                temperature: Some(0.3),
                max_output_tokens: Some(500),
                ..Default::default()
            };

            let req = LlmRequest {
                messages: vec![
                    LlmMessage {
                        role: "system".to_string(),
                        content: "You are the Proponent agent. Advocate persuasively and logically for the proposition.".to_string(),
                    },
                    LlmMessage {
                        role: "user".to_string(),
                        content: prop_prompt,
                    },
                ],
                config: cfg.clone(),
                tools: None,
                response_schema: None,
            };

            let prop_res = provider.generate(&req).await?;
            let prop_text = prop_res.content;

            Self::post_swarm_message(
                &app_state,
                &project_id,
                &conversation_id,
                &proponent_id,
                &format!("[Debate Round {} - Proponent]: {}", r, prop_text),
            )
            .await?;

            transcript.push((format!("Proponent (@{})", proponent_id), prop_text.clone()));

            // 2. Critic Turn
            let critic_prompt = format!(
                "Debate Round {}/{} on '{}'. The Proponent argued:\n\"{}\"\nCritique this argument rigorously. Identify logical fallacies, edge-case risks, scalability bottlenecks, and invalid assumptions.",
                r, rounds, topic, prop_text
            );

            let critic_req = LlmRequest {
                messages: vec![
                    LlmMessage {
                        role: "system".to_string(),
                        content: "You are the Adversarial Critic agent. Scrutinize the proponent's claims with skepticism.".to_string(),
                    },
                    LlmMessage {
                        role: "user".to_string(),
                        content: critic_prompt,
                    },
                ],
                config: cfg,
                tools: None,
                response_schema: None,
            };

            let critic_res = provider.generate(&critic_req).await?;
            let critic_text = critic_res.content;

            Self::post_swarm_message(
                &app_state,
                &project_id,
                &conversation_id,
                &opponent_id,
                &format!("[Debate Round {} - Critic]: {}", r, critic_text),
            )
            .await?;

            transcript.push((format!("Critic (@{})", opponent_id), critic_text.clone()));
            last_critic_critique = critic_text;
        }

        // 3. Final Consensus Synthesis
        let mut debate_summary = String::new();
        for (speaker, text) in &transcript {
            debate_summary.push_str(&format!("{}: {}\n\n", speaker, text));
        }

        let cfg = ModelConfig {
            provider: provider.name().to_string(),
            temperature: Some(0.2),
            max_output_tokens: Some(800),
            ..Default::default()
        };

        let req = LlmRequest {
            messages: vec![
                LlmMessage {
                    role: "system".to_string(),
                    content: "You are an executive consensus judge. Synthesize an actionable resolution from this multi-agent debate.".to_string(),
                },
                LlmMessage {
                    role: "user".to_string(),
                    content: format!(
                        "Synthesize a vetted consensus decision on '{}' following this adversarial debate:\n\n{}",
                        topic, debate_summary
                    ),
                }
            ],
            config: cfg,
            tools: None,
            response_schema: None,
        };

        let synth_res = provider.generate(&req).await?;
        let synthesis = synth_res.content;

        Ok(SwarmResult {
            pattern: "Debate".to_string(),
            final_output: synthesis,
            transcript,
        })
    }

    async fn spawn_worker_execution(
        app_state: &Arc<AppState>,
        project_id: &ProjectId,
        conversation_id: &ConversationId,
        worker_id: &AgentInstanceId,
        supervisor_id: &AgentInstanceId,
        task_text: &str,
    ) -> Result<trans4mers_domain::ids::ExecutionId, Trans4mersError> {
        use trans4mers_domain::ids::{ActorId, ChannelId, ExecutionId, MessageId};
        let exec_id = ExecutionId::new();
        let db = app_state
            .get_project_db(project_id)
            .ok_or_else(|| Trans4mersError::Database("Project DB not found".to_string()))?;

        let sub_agent_msg = trans4mers_domain::message::Message {
            id: MessageId::new(),
            conversation_id: *conversation_id,
            channel_id: ChannelId::from_uuid(*worker_id.as_uuid()),
            sender: trans4mers_domain::actor::Actor::Agent(*supervisor_id),
            content: task_text.to_string(),
            mentions: vec![],
            message_kind: trans4mers_domain::message::MessageKind::Chat,
            thread_id: None,
            attachments: vec![],
            requires_approval: false,
            approval_id: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let inbox_msg_id = MessageId::new().to_string();
        let inbox_event = trans4mers_domain::event::DomainEvent::InboxMessageQueued {
            message_id: inbox_msg_id,
            recipient_agent_id: *worker_id,
            sender_actor_id: format!("agent:{}", supervisor_id),
            payload: serde_json::to_string(&sub_agent_msg).unwrap_or_default(),
        };

        let inbox_envelope = db.with_write_tx(|tx| {
            tx.execute(
                "INSERT INTO agent_executions (id, agent_instance_id, conversation_id, status, generation, current_step, max_steps, updated_at)
                 VALUES (?1, ?2, ?3, 'Pending', 0, 0, 25, ?4)",
                rusqlite::params![exec_id.as_str(), worker_id.as_str(), conversation_id.as_str(), chrono::Utc::now().to_rfc3339()]
            )?;
            crate::cqrs::commit_event(tx, inbox_event, ActorId::new())
        })?;

        let bus = app_state.get_event_bus(project_id);
        let env_inbox_arc = Arc::new(inbox_envelope);
        let _ = bus.publish(env_inbox_arc.clone());
        let _ = app_state.global_event_bus.publish(env_inbox_arc);

        app_state.scheduler.queue(exec_id, *project_id);

        Ok(exec_id)
    }

    /// Executes a supervisor workflow: decomposing a macro goal and delegating milestones to workers
    pub async fn run_supervisor(
        app_state: Arc<AppState>,
        project_id: ProjectId,
        conversation_id: ConversationId,
        goal: String,
        supervisor_id: AgentInstanceId,
        worker_ids: Vec<AgentInstanceId>,
    ) -> Result<SwarmResult, Trans4mersError> {
        info!(goal = %goal, workers = worker_ids.len(), "Starting supervisor orchestration");

        let provider = app_state.provider_registry.get_default().map_err(|e| {
            Trans4mersError::Internal(format!(
                "No LLM provider available for supervisor orchestration: {}",
                e
            ))
        })?;
        let mut transcript = Vec::new();

        // 1. Supervisor decomposes macro goal via LLM
        let cfg = ModelConfig {
            provider: provider.name().to_string(),
            temperature: Some(0.2),
            max_output_tokens: Some(800),
            ..Default::default()
        };

        let req = LlmRequest {
            messages: vec![
                LlmMessage {
                    role: "system".to_string(),
                    content: "You are the Lead Supervisor agent. Break down the user's macro goal into concrete, actionable milestones for specialized workers.".to_string(),
                },
                LlmMessage {
                    role: "user".to_string(),
                    content: format!(
                        "Decompose this macro goal into exactly {} distinct sequential milestones:\n\"{}\"",
                        worker_ids.len().max(1),
                        goal
                    ),
                }
            ],
            config: cfg,
            tools: None,
            response_schema: None,
        };

        let plan_res = provider.generate(&req).await?;
        let plan_text = plan_res.content;

        Self::post_swarm_message(
            &app_state,
            &project_id,
            &conversation_id,
            &supervisor_id,
            &plan_text,
        )
        .await?;
        transcript.push((
            format!("Supervisor (@{})", supervisor_id),
            plan_text.clone(),
        ));

        // 2. Delegate milestone to each worker
        let mut exec_ids = Vec::new();
        for (i, worker_id) in worker_ids.iter().enumerate() {
            let milestone_text = format!(
                "Milestone {}/{} assigned to @{}: Execute task component for goal: '{}'",
                i + 1,
                worker_ids.len(),
                worker_id,
                goal
            );
            let exec_id = Self::spawn_worker_execution(
                &app_state,
                &project_id,
                &conversation_id,
                worker_id,
                &supervisor_id,
                &milestone_text,
            )
            .await?;
            exec_ids.push(exec_id);
            transcript.push((format!("Worker (@{})", worker_id), milestone_text));
        }

        // 3. Wait for results
        let db = app_state
            .get_project_db(&project_id)
            .ok_or_else(|| Trans4mersError::Database("DB not found".to_string()))?;
        let wait_start = std::time::Instant::now();
        let max_wait = std::time::Duration::from_secs(60);
        let mut final_status_map = std::collections::HashMap::new();

        loop {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            let mut completed_count = 0;
            let _ = db.with_read_conn(|conn| {
                for eid in &exec_ids {
                    if let Ok(status) = conn.query_row(
                        "SELECT status FROM agent_executions WHERE id = ?1",
                        rusqlite::params![eid.as_str()],
                        |row| row.get::<_, String>(0),
                    ) {
                        if status == "Completed" || status == "Failed" || status == "Cancelled" {
                            completed_count += 1;
                        }
                        final_status_map.insert(*eid, status);
                    }
                }
                Ok(())
            });
            if completed_count == exec_ids.len() || wait_start.elapsed() >= max_wait {
                break;
            }
        }

        let mut status_summary = String::new();
        let mut all_completed = true;
        for (i, eid) in exec_ids.iter().enumerate() {
            let status = final_status_map
                .get(eid)
                .cloned()
                .unwrap_or_else(|| "Unknown".to_string());
            if status != "Completed" {
                all_completed = false;
            }
            status_summary.push_str(&format!(
                "- Milestone #{}: Execution `{}` -> **{}**\n",
                i + 1,
                eid,
                status
            ));
        }

        let overall_status = if all_completed && !exec_ids.is_empty() {
            "All milestones completed successfully."
        } else if wait_start.elapsed() >= max_wait {
            "Execution timeout reached before all milestones completed."
        } else {
            "Some milestones failed or were cancelled."
        };

        let final_output = format!(
            "### Supervisor Swarm Execution Report\n**Goal:** {}\n**Workers Coordinated:** {}\n\n**Decomposition Plan:**\n{}\n\n**Milestone Outcomes:**\n{}\n**Overall Status:** {}",
            goal,
            worker_ids.len(),
            plan_text,
            status_summary,
            overall_status
        );

        Ok(SwarmResult {
            pattern: "Supervisor".to_string(),
            final_output,
            transcript,
        })
    }

    /// Executes a fan-out swarm where multiple worker agents execute subtasks concurrently
    /// and their findings are collated by the coordinator.
    pub async fn run_fanout(
        app_state: Arc<AppState>,
        project_id: ProjectId,
        conversation_id: ConversationId,
        subtasks: Vec<String>,
        coordinator_id: AgentInstanceId,
        worker_ids: Vec<AgentInstanceId>,
    ) -> Result<SwarmResult, Trans4mersError> {
        if worker_ids.is_empty() {
            return Err(Trans4mersError::Internal(
                "At least one worker is required for fan-out".to_string(),
            ));
        }

        info!(
            subtasks = subtasks.len(),
            workers = worker_ids.len(),
            "Starting fan-out swarm execution"
        );

        let mut transcript = Vec::new();

        // 1. Coordinator announcement
        let announce = format!(
            "Coordinator (@{}) initiating fan-out execution across {} workers for {} subtasks.",
            coordinator_id,
            worker_ids.len(),
            subtasks.len()
        );
        Self::post_swarm_message(
            &app_state,
            &project_id,
            &conversation_id,
            &coordinator_id,
            &announce,
        )
        .await?;

        // 2. Parallel worker dispatch
        let mut exec_ids = Vec::new();
        let mut worker_task_map = Vec::new();
        for (i, task) in subtasks.iter().enumerate() {
            let worker_id = worker_ids[i % worker_ids.len()];
            let exec_id = Self::spawn_worker_execution(
                &app_state,
                &project_id,
                &conversation_id,
                &worker_id,
                &coordinator_id,
                task,
            )
            .await?;
            exec_ids.push(exec_id);
            worker_task_map.push((worker_id, task.clone()));
            transcript.push((
                format!("Worker (@{})", worker_id),
                format!("Dispatched: {}", task),
            ));
        }

        // 3. Wait for results
        let db = app_state
            .get_project_db(&project_id)
            .ok_or_else(|| Trans4mersError::Database("DB not found".to_string()))?;
        let wait_start = std::time::Instant::now();
        let max_wait = std::time::Duration::from_secs(60);
        let mut final_status_map = std::collections::HashMap::new();

        loop {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            let mut completed_count = 0;
            let _ = db.with_read_conn(|conn| {
                for eid in &exec_ids {
                    if let Ok(status) = conn.query_row(
                        "SELECT status FROM agent_executions WHERE id = ?1",
                        rusqlite::params![eid.as_str()],
                        |row| row.get::<_, String>(0),
                    ) {
                        if status == "Completed" || status == "Failed" || status == "Cancelled" {
                            completed_count += 1;
                        }
                        final_status_map.insert(*eid, status);
                    }
                }
                Ok(())
            });
            if completed_count == exec_ids.len() || wait_start.elapsed() >= max_wait {
                break;
            }
        }

        let mut final_output = format!(
            "### Swarm Fan-Out Execution Report\n**Subtasks Processed:** {}\n**Workers Engaged:** {}\n\n",
            subtasks.len(),
            worker_ids.len()
        );
        for (i, (w, task)) in worker_task_map.iter().enumerate() {
            let eid = &exec_ids[i];
            let status = final_status_map
                .get(eid)
                .cloned()
                .unwrap_or_else(|| "Unknown".to_string());
            final_output.push_str(&format!(
                "- **Worker:** `{}` (Exec `{}`)\n  **Task:** {}\n  **Status:** {}\n\n",
                w, eid, task, status
            ));
        }

        Ok(SwarmResult {
            pattern: "FanOut".to_string(),
            final_output,
            transcript,
        })
    }

    /// Helper to commit and broadcast a message from an agent to a conversation channel
    async fn post_swarm_message(
        app_state: &AppState,
        project_id: &ProjectId,
        conversation_id: &ConversationId,
        sender_agent_id: &AgentInstanceId,
        content: &str,
    ) -> Result<(), Trans4mersError> {
        let db = app_state
            .get_project_db(project_id)
            .ok_or_else(|| Trans4mersError::Database("Project DB not found".to_string()))?;

        let msg = Message {
            id: trans4mers_domain::ids::MessageId::new(),
            conversation_id: *conversation_id,
            channel_id: trans4mers_domain::ids::ChannelId::from(conversation_id.as_str()),
            thread_id: None,
            sender: trans4mers_domain::actor::Actor::Agent(*sender_agent_id),
            content: content.to_string(),
            message_kind: MessageKind::Chat,
            mentions: vec![],
            attachments: vec![],
            requires_approval: false,
            approval_id: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let event = DomainEvent::MessageSent { message: msg };
        let envelope = db.with_write_tx(|tx| {
            crate::cqrs::commit_event(tx, event, ActorId::from_uuid(*sender_agent_id.as_uuid()))
        })?;

        let env_arc = Arc::new(envelope);
        let _ = app_state.get_event_bus(project_id).publish(env_arc.clone());
        let _ = app_state.global_event_bus.publish(env_arc);

        Ok(())
    }
}
