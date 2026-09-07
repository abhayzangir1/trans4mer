use rusqlite::{Transaction, params};
use trans4mers_domain::agent::AgentInstance;
use trans4mers_domain::approval::{Approval, ApprovalStatus};
use trans4mers_domain::conversation::Conversation;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::event::{DomainEvent, EventEnvelope};
use trans4mers_domain::execution::{AgentExecution, Checkpoint, ExecutionPhase};
use trans4mers_domain::project::Project;
use trans4mers_storage::repos::{
    agent_inst_repo, approval_repo, conversation_repo, execution_repo, message_repo, project_repo,
};
use uuid::Uuid;

pub struct EventProjector;

impl EventProjector {
    pub fn project_event(
        tx: &Transaction,
        envelope: &EventEnvelope,
    ) -> Result<(), Trans4mersError> {
        match &envelope.event {
            DomainEvent::ProjectCreated {
                project_id,
                name,
                workspace_path,
            } => {
                let project = Project {
                    id: *project_id,
                    name: name.clone(),
                    workspace_path: workspace_path.clone(),
                    description: None,
                    global_instructions: None,
                    settings: Default::default(),
                    embedding_dimensions: 768, // DEFAULT_EMBEDDING_DIMENSIONS: Currently hardcoded as event doesn't contain it. To make configurable, add it to DomainEvent::ProjectCreated and pass from AppConfig.
                    created_at: envelope.created_at,
                    updated_at: envelope.created_at,
                };
                project_repo::insert_project(tx, &project)?;
            }

            DomainEvent::ConversationCreated {
                conversation_id,
                project_id,
                title,
            } => {
                let conv = Conversation {
                    id: *conversation_id,
                    project_id: *project_id,
                    title: title.clone(),
                    status: trans4mers_domain::conversation::ConversationStatus::Active,
                    settings: trans4mers_domain::conversation::ConversationSettings::default(),
                    created_at: envelope.created_at,
                    updated_at: envelope.created_at,
                };
                conversation_repo::insert_conversation(tx, &conv)?;
            }

            DomainEvent::AgentInstanceCreated {
                agent_instance_id,
                definition_id,
                project_id,
            } => {
                let agent = AgentInstance {
                    id: *agent_instance_id,
                    project_id: *project_id,
                    definition_id: definition_id.clone(),
                    parent_instance_id: None,
                    status: trans4mers_domain::state::AgentStatus::Active,
                    capabilities: vec![
                        trans4mers_domain::tool::Capability::FilesystemRead,
                        trans4mers_domain::tool::Capability::FilesystemWrite,
                        trans4mers_domain::tool::Capability::ShellExecute,
                        trans4mers_domain::tool::Capability::AgentSpawn,
                    ],
                    model_config_override: None,
                    depth_level: 0,
                    created_at: envelope.created_at,
                    updated_at: envelope.created_at,
                };
                agent_inst_repo::insert_agent_instance(tx, &agent)?;
            }

            DomainEvent::AgentStatusChanged {
                agent_instance_id,
                new_status,
                ..
            } => {
                tx.execute(
                    "UPDATE agent_instances SET status = ?1, updated_at = ?2 WHERE id = ?3",
                    params![
                        new_status.to_string(),
                        envelope.created_at.to_rfc3339(),
                        agent_instance_id.as_str()
                    ],
                )
                .map_err(|e| Trans4mersError::Database(e.to_string()))?;
            }

            DomainEvent::ExecutionStarted {
                execution_id,
                agent_instance_id,
            } => {
                let updated = tx.execute(
                    "UPDATE agent_executions SET status = 'Running', started_at = COALESCE(started_at, ?1), updated_at = ?1 WHERE id = ?2",
                    params![envelope.created_at.to_rfc3339(), execution_id.as_str()]
                ).map_err(|e| Trans4mersError::Database(e.to_string()))?;

                if updated == 0 {
                    let exec = AgentExecution {
                        id: *execution_id,
                        agent_instance_id: *agent_instance_id,
                        task_id: None,
                        conversation_id: trans4mers_domain::ids::ConversationId::new(),
                        status: trans4mers_domain::state::ExecutionStatus::Running,
                        generation: 1,
                        current_step: 0,
                        max_steps: 50,
                        trigger_message_id: None,
                        started_at: Some(envelope.created_at),
                        updated_at: envelope.created_at,
                        completed_at: None,
                    };
                    execution_repo::insert_execution(tx, &exec)?;
                }
            }

            DomainEvent::ExecutionStepCompleted {
                execution_id,
                step_number,
            } => {
                tx.execute(
                    "UPDATE agent_executions SET current_step = ?1, updated_at = ?2 WHERE id = ?3",
                    params![
                        step_number,
                        envelope.created_at.to_rfc3339(),
                        execution_id.as_str()
                    ],
                )
                .map_err(|e| Trans4mersError::Database(e.to_string()))?;
            }

            DomainEvent::ExecutionStatusChanged {
                execution_id,
                new_status,
                ..
            } => {
                tx.execute(
                    "UPDATE agent_executions SET status = ?1, updated_at = ?2 WHERE id = ?3",
                    params![
                        new_status.to_string(),
                        envelope.created_at.to_rfc3339(),
                        execution_id.as_str()
                    ],
                )
                .map_err(|e| Trans4mersError::Database(e.to_string()))?;
            }

            DomainEvent::ExecutionCompleted { execution_id } => {
                tx.execute(
                    "UPDATE agent_executions SET status = 'Completed', completed_at = ?1, updated_at = ?1 WHERE id = ?2",
                    params![envelope.created_at.to_rfc3339(), execution_id.as_str()]
                ).map_err(|e| Trans4mersError::Database(e.to_string()))?;
            }

            DomainEvent::ExecutionFailed { execution_id, .. } => {
                tx.execute(
                    "UPDATE agent_executions SET status = 'Failed', completed_at = ?1, updated_at = ?1 WHERE id = ?2",
                    params![envelope.created_at.to_rfc3339(), execution_id.as_str()]
                ).map_err(|e| Trans4mersError::Database(e.to_string()))?;
            }

            DomainEvent::CheckpointSaved {
                execution_id,
                step_number,
                phase,
            } => {
                let cp = Checkpoint {
                    id: trans4mers_domain::ids::CheckpointId::new(),
                    execution_id: *execution_id,
                    generation: 1,
                    step_number: *step_number,
                    execution_status: trans4mers_domain::state::ExecutionStatus::Checkpointing,
                    execution_phase: std::str::FromStr::from_str(phase)
                        .unwrap_or(ExecutionPhase::LlmGeneration),
                    inbox_cursor: None,
                    pending_tool_state: None,
                    last_event_sequence: envelope.sequence_id,
                    context_snapshot: None,
                    created_at: envelope.created_at,
                };
                execution_repo::upsert_checkpoint(tx, &cp)?;
            }

            DomainEvent::MessageSent { message } | DomainEvent::MessageCreated { message } => {
                message_repo::insert_message(tx, message)?;
            }

            DomainEvent::InboxMessageQueued {
                message_id,
                recipient_agent_id,
                sender_actor_id,
                payload,
            } => {
                tx.execute(
                    "INSERT INTO inbox_messages (id, recipient_agent_id, sender_actor_id, payload, delivery_state, created_at)
                     VALUES (?1, ?2, ?3, ?4, 'QUEUED', ?5)",
                    params![
                        message_id.as_str(),
                        recipient_agent_id.as_str(),
                        sender_actor_id,
                        payload,
                        envelope.created_at.to_rfc3339()
                    ]
                ).map_err(|e| Trans4mersError::Database(e.to_string()))?;
            }

            DomainEvent::InboxMessageClaimed {
                message_id,
                agent_id,
            } => {
                tx.execute(
                    "UPDATE inbox_messages SET delivery_state = 'CLAIMED', claimed_at = ?1
                     WHERE recipient_agent_id = ?2 AND id = ?3",
                    params![
                        envelope.created_at.to_rfc3339(),
                        agent_id.as_str(),
                        message_id
                    ],
                )
                .map_err(|e| Trans4mersError::Database(e.to_string()))?;
            }

            DomainEvent::InboxMessageAcked {
                message_id,
                agent_id,
            } => {
                tx.execute(
                    "UPDATE inbox_messages SET delivery_state = 'ACKED'
                     WHERE recipient_agent_id = ?1 AND id = ?2",
                    params![agent_id.as_str(), message_id],
                )
                .map_err(|e| Trans4mersError::Database(e.to_string()))?;
            }

            DomainEvent::AgentSpawned {
                agent_id,
                project_id,
                definition_id,
                parent_id,
            } => {
                let agent = AgentInstance {
                    id: *agent_id,
                    project_id: *project_id,
                    definition_id: std::str::FromStr::from_str(definition_id)
                        .unwrap_or_else(|_| Uuid::new_v4().into()),
                    parent_instance_id: parent_id
                        .as_ref()
                        .and_then(|id| std::str::FromStr::from_str(id).ok()),
                    status: trans4mers_domain::state::AgentStatus::Active,
                    capabilities: vec![
                        trans4mers_domain::tool::Capability::FilesystemRead,
                        trans4mers_domain::tool::Capability::FilesystemWrite,
                        trans4mers_domain::tool::Capability::ShellExecute,
                        trans4mers_domain::tool::Capability::AgentSpawn,
                    ],
                    model_config_override: None,
                    depth_level: if parent_id.is_some() { 1 } else { 0 },
                    created_at: envelope.created_at,
                    updated_at: envelope.created_at,
                };
                agent_inst_repo::insert_agent_instance(tx, &agent)?;
            }

            DomainEvent::PendingApproval {
                approval_id,
                execution_id,
                conversation_id,
                agent_id,
                capability,
                tool_name,
                payload,
                risk_level,
                arguments_hash,
            } => {
                let parsed_id = std::str::FromStr::from_str(approval_id)
                    .unwrap_or_else(|_| trans4mers_domain::ids::ApprovalId::new());
                let approval = Approval {
                    id: parsed_id,
                    execution_id: *execution_id,
                    agent_instance_id: *agent_id,
                    conversation_id: *conversation_id,
                    capability: std::str::FromStr::from_str(capability)
                        .unwrap_or(trans4mers_domain::tool::Capability::ShellExecute),
                    tool_name: Some(tool_name.clone()),
                    action_description: format!("Requesting execution of {}", tool_name),
                    arguments_summary: payload.clone(),
                    arguments_hash: arguments_hash.clone(),
                    risk_level: std::str::FromStr::from_str(risk_level)
                        .unwrap_or(trans4mers_domain::tool::RiskLevel::Medium),
                    status: ApprovalStatus::Pending,
                    human_feedback: None,
                    requested_at: envelope.created_at,
                    expires_at: Approval::default_expiry(),
                    resolved_at: None,
                };
                approval_repo::insert_approval(tx, &approval)?;
            }

            DomainEvent::ApprovalResolved {
                approval_id,
                approved,
                feedback,
            } => {
                let status = if *approved {
                    ApprovalStatus::Approved
                } else {
                    ApprovalStatus::Rejected
                };
                if let Ok(app_id) = std::str::FromStr::from_str(approval_id) {
                    approval_repo::resolve_approval(tx, &app_id, status, feedback.clone())?;
                }
            }

            DomainEvent::ToolExecuted {
                execution_id,
                tool_name,
                success,
            } => {
                tx.execute(
                    "INSERT INTO tool_executions (id, execution_id, tool_name, success, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![
                        Uuid::new_v4().to_string(),
                        execution_id.as_str(),
                        tool_name,
                        success,
                        envelope.created_at.to_rfc3339()
                    ],
                )
                .map_err(|e| Trans4mersError::Database(e.to_string()))?;
            }

            DomainEvent::WorkflowRunStarted {
                run_id,
                workflow_id,
            } => {
                tx.execute(
                    "INSERT INTO workflow_runs (id, workflow_id, status, current_node_id, started_at)
                     VALUES (?1, ?2, 'Running', NULL, ?3)",
                    rusqlite::params![
                        run_id.as_str(),
                        workflow_id.as_str(),
                        envelope.created_at.to_rfc3339()
                    ]
                ).map_err(|e| Trans4mersError::Database(e.to_string()))?;
            }

            DomainEvent::WorkflowNodeStarted { run_id, node_id } => {
                tx.execute(
                    "UPDATE workflow_runs SET current_node_id = ?1 WHERE id = ?2",
                    rusqlite::params![node_id.as_str(), run_id.as_str()],
                )
                .map_err(|e| Trans4mersError::Database(e.to_string()))?;
            }

            DomainEvent::WorkflowNodeCompleted { run_id, node_id: _ } => {
                tx.execute(
                    "UPDATE workflow_runs SET current_node_id = NULL WHERE id = ?1",
                    rusqlite::params![run_id.as_str()],
                )
                .map_err(|e| Trans4mersError::Database(e.to_string()))?;
            }

            DomainEvent::WorkflowRunCompleted { run_id } => {
                tx.execute(
                    "UPDATE workflow_runs SET status = 'Completed', completed_at = ?1 WHERE id = ?2",
                    rusqlite::params![envelope.created_at.to_rfc3339(), run_id.as_str()]
                ).map_err(|e| Trans4mersError::Database(e.to_string()))?;
            }

            DomainEvent::WorkflowRunFailed { run_id, reason: _ } => {
                tx.execute(
                    "UPDATE workflow_runs SET status = 'Failed', completed_at = ?1 WHERE id = ?2",
                    rusqlite::params![envelope.created_at.to_rfc3339(), run_id.as_str()],
                )
                .map_err(|e| Trans4mersError::Database(e.to_string()))?;
            }

            DomainEvent::MemoryTierPromoted {
                memory_id,
                new_tier,
                ..
            } => {
                let _ = tx.execute(
                    "UPDATE project_memories SET tier = ?1, updated_at = ?2 WHERE id = ?3",
                    rusqlite::params![new_tier, envelope.created_at.to_rfc3339(), memory_id],
                );
            }

            DomainEvent::DiffReviewResolved {
                diff_id, decision, ..
            } => {
                let _ = tx.execute(
                    "UPDATE action_diffs SET decision = ?1, updated_at = ?2, resolved_at = ?2 WHERE id = ?3",
                    rusqlite::params![decision, envelope.created_at.to_rfc3339(), diff_id],
                );
            }

            DomainEvent::DiffReviewRequested { .. }
            | DomainEvent::SkillProposed { .. }
            | DomainEvent::CostAlertFired { .. }
            | DomainEvent::MessageQueued { .. }
            | DomainEvent::PolicyEvaluated { .. }
            | DomainEvent::ApprovalExpired { .. }
            | DomainEvent::MemoryStored { .. }
            | DomainEvent::RuleLearned { .. }
            | DomainEvent::ArtifactCreated { .. }
            | DomainEvent::TerminalOutput { .. }
            | DomainEvent::FileModifiedByHuman { .. }
            | DomainEvent::ArtifactCommentAdded { .. }
            | DomainEvent::TelegramMessageReceived { .. }
            | DomainEvent::McpFrameLogged { .. }
            | DomainEvent::TextDelta { .. } => {
                // These are safely appended to the event log.
                // They either map to tables updated asynchronously (like artifacts/memory via separate engines),
                // or are ephemeral. The `EventProjector` ensures strictly relational updates.
            }
        }
        Ok(())
    }
}
