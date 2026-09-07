use crate::app_state::AppState;
use rusqlite::params;
use std::sync::Arc;
use tokio::task::JoinHandle;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::event::DomainEvent;
use trans4mers_domain::ids::{ActorId, ProjectId, WorkflowId, WorkflowRunId};
use trans4mers_domain::workflow::{Workflow, WorkflowNodeType};

pub struct WorkflowEngine {
    state: Arc<AppState>,
}

impl WorkflowEngine {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    /// Fetches a workflow by ID
    fn get_workflow(
        &self,
        project_id: &ProjectId,
        workflow_id: &WorkflowId,
    ) -> Result<Workflow, Trans4mersError> {
        let db = self
            .state
            .get_project_db(project_id)
            .ok_or_else(|| Trans4mersError::Database("Project DB not found".to_string()))?;

        db.with_read_conn(|conn| {
            let mut stmt = conn.prepare("SELECT id, conversation_id, name, description, nodes, edges, created_at, updated_at FROM workflows WHERE id = ?1")
                .map_err(|e| Trans4mersError::Database(e.to_string()))?;
            let mut iter = stmt.query_map(params![workflow_id.as_str()], |row| {
                let nodes_json: String = row.get(4)?;
                let edges_json: String = row.get(5)?;
                let id_str = row.get::<_, String>(0)?;
                let convo_id_str = row.get::<_, String>(1)?;
                let created_str = row.get::<_, String>(6)?;
                let updated_str = row.get::<_, String>(7)?;
                Ok(Workflow {
                    id: std::str::FromStr::from_str(&id_str).unwrap_or_else(|_| trans4mers_domain::ids::WorkflowId::new()),
                    conversation_id: std::str::FromStr::from_str(&convo_id_str).unwrap_or_else(|_| trans4mers_domain::ids::ConversationId::new()),
                    name: row.get(2)?,
                    description: row.get(3)?,
                    nodes: serde_json::from_str(&nodes_json).unwrap_or_default(),
                    edges: serde_json::from_str(&edges_json).unwrap_or_default(),
                    created_at: trans4mers_storage::datetime_util::parse_db_datetime(&created_str),
                    updated_at: trans4mers_storage::datetime_util::parse_db_datetime(&updated_str),
                })
            }).map_err(|e| Trans4mersError::Database(e.to_string()))?;

            let item = iter.next().ok_or_else(|| Trans4mersError::Database("Workflow not found".to_string()))?;
            item.map_err(|e| Trans4mersError::Database(e.to_string()))
        })
    }

    /// Spawns an asynchronous execution loop for a workflow run
    pub async fn execute_workflow_run(
        &self,
        project_id: ProjectId,
        workflow_id: WorkflowId,
        run_id: WorkflowRunId,
    ) -> JoinHandle<()> {
        let engine = Self::new(Arc::clone(&self.state));

        tokio::spawn(async move {
            if let Err(e) = engine.process_dag(project_id, workflow_id, run_id).await {
                let _ = engine
                    .emit_event(
                        &project_id,
                        DomainEvent::WorkflowRunFailed {
                            run_id,
                            reason: e.to_string(),
                        },
                    )
                    .await;
            }
        })
    }

    async fn process_dag(
        &self,
        project_id: ProjectId,
        workflow_id: WorkflowId,
        run_id: WorkflowRunId,
    ) -> Result<(), Trans4mersError> {
        self.emit_event(
            &project_id,
            DomainEvent::WorkflowRunStarted {
                run_id,
                workflow_id,
            },
        )
        .await?;

        let workflow = self.get_workflow(&project_id, &workflow_id)?;

        let db = self.state.get_project_db(&project_id).unwrap();
        let mut recovered_node_id: Option<String> = None;
        let _ = db.with_read_conn(|conn| {
            if let Ok(node_id) = conn.query_row(
                "SELECT current_node_id FROM workflow_runs WHERE id = ?1",
                rusqlite::params![run_id.as_str()],
                |row| row.get::<_, Option<String>>(0),
            ) {
                recovered_node_id = node_id;
            }
            Ok(())
        });

        // Initialize active_nodes queue
        let mut active_nodes: Vec<trans4mers_domain::ids::WorkflowNodeId> =
            if let Some(n) = recovered_node_id {
                if n.starts_with('[') {
                    serde_json::from_str(&n).unwrap_or_else(|_| {
                        std::str::FromStr::from_str(&n)
                            .map(|id| vec![id])
                            .unwrap_or_default()
                    })
                } else {
                    std::str::FromStr::from_str(&n)
                        .map(|id| vec![id])
                        .unwrap_or_default()
                }
            } else {
                vec![
                    workflow
                        .nodes
                        .iter()
                        .find(|n| n.node_type == WorkflowNodeType::Start)
                        .map(|n| n.id)
                        .ok_or_else(|| {
                            Trans4mersError::Internal("Workflow has no start node".to_string())
                        })?,
                ]
            };

        while !active_nodes.is_empty() {
            let current_node_id = active_nodes.remove(0);

            // Checkpoint state
            let _ = db.with_write_tx(|tx| {
                let mut checkpoint = active_nodes.clone();
                checkpoint.insert(0, current_node_id);
                let checkpoint_json = serde_json::to_string(&checkpoint).unwrap_or_default();
                tx.execute(
                    "INSERT INTO workflow_runs (id, workflow_id, status, current_node_id) VALUES (?1, ?2, 'Running', ?3) ON CONFLICT(id) DO UPDATE SET status = 'Running', current_node_id = ?3",
                    rusqlite::params![run_id.as_str(), workflow_id.as_str(), checkpoint_json]
                )?;
                Ok(())
            });

            self.emit_event(
                &project_id,
                DomainEvent::WorkflowNodeStarted {
                    run_id,
                    node_id: current_node_id,
                },
            )
            .await?;

            let current_node = workflow
                .nodes
                .iter()
                .find(|n| n.id == current_node_id)
                .ok_or_else(|| Trans4mersError::Internal("Node not found in graph".to_string()))?
                .clone();

            match current_node.node_type {
                WorkflowNodeType::Start => {}
                WorkflowNodeType::End => {
                    self.emit_event(
                        &project_id,
                        DomainEvent::WorkflowNodeCompleted {
                            run_id,
                            node_id: current_node_id,
                        },
                    )
                    .await?;
                    continue;
                }
                WorkflowNodeType::AgentTask => {
                    if let Some(def_id) = &current_node.agent_definition_id {
                        let agent_id = trans4mers_domain::ids::AgentInstanceId::new();
                        let exec_id = trans4mers_domain::ids::ExecutionId::new();

                        self.emit_event(
                            &project_id,
                            DomainEvent::AgentSpawned {
                                agent_id,
                                project_id,
                                definition_id: def_id.to_string(),
                                parent_id: None,
                            },
                        )
                        .await?;

                        let _ = db.with_write_tx(|tx| {
                            tx.execute(
                                "INSERT INTO agent_executions (id, agent_instance_id, conversation_id, status, current_step, generation, created_at, updated_at)
                                 VALUES (?1, ?2, ?3, 'RUNNING', 0, 1, ?4, ?5)",
                                rusqlite::params![
                                    exec_id.as_str(),
                                    agent_id.as_str(),
                                    workflow.conversation_id.as_str(),
                                    chrono::Utc::now().to_rfc3339(),
                                    chrono::Utc::now().to_rfc3339()
                                ]
                            ).map_err(|e| trans4mers_domain::error::Trans4mersError::Database(e.to_string()))
                        });

                        if let Some(desc) = &current_node.task_description {
                            let msg = trans4mers_domain::message::Message {
                                id: trans4mers_domain::ids::MessageId::new(),
                                conversation_id: workflow.conversation_id,
                                channel_id: trans4mers_domain::ids::ChannelId::from_uuid(
                                    *agent_id.as_uuid(),
                                ),
                                thread_id: None,
                                sender: trans4mers_domain::actor::Actor::System,
                                content: desc.clone(),
                                message_kind: trans4mers_domain::message::MessageKind::Chat,
                                mentions: vec![],
                                attachments: vec![],
                                requires_approval: false,
                                approval_id: None,
                                created_at: chrono::Utc::now(),
                                updated_at: chrono::Utc::now(),
                            };
                            self.emit_event(&project_id, DomainEvent::MessageSent { message: msg })
                                .await?;
                        }

                        // Actually queue execution in the Scheduler
                        self.state.scheduler.queue(exec_id, project_id);
                    }
                }
                WorkflowNodeType::HumanReview => {
                    let is_approved = db.with_read_conn(|conn| {
                        let status: Result<String, _> = conn.query_row(
                            "SELECT status FROM approvals WHERE execution_id = ?1 AND tool_name = 'HumanReview' AND arguments_summary = ?2 ORDER BY created_at DESC LIMIT 1",
                            rusqlite::params![run_id.as_str(), current_node.name],
                            |row| row.get(0)
                        );
                        Ok(status.unwrap_or_else(|_| "Pending".to_string()) == "Approved")
                    }).unwrap_or(false);

                    if !is_approved {
                        let approval_id = trans4mers_domain::ids::ApprovalId::new();
                        self.emit_event(
                            &project_id,
                            DomainEvent::PendingApproval {
                                approval_id: approval_id.to_string(),
                                execution_id: trans4mers_domain::ids::ExecutionId::from_uuid(
                                    *run_id.as_uuid(),
                                ),
                                conversation_id: workflow.conversation_id,
                                agent_id: trans4mers_domain::ids::AgentInstanceId::new(),
                                capability: "Workflow".to_string(),
                                tool_name: "HumanReview".to_string(),
                                payload: current_node.name.clone(),
                                risk_level: "High".to_string(),
                                arguments_hash: None,
                            },
                        )
                        .await?;

                        let _ = self.state.get_project_db(&project_id).unwrap().with_write_tx(|tx| {
                            let mut checkpoint = active_nodes.clone();
                            checkpoint.insert(0, current_node_id);
                            let checkpoint_json = serde_json::to_string(&checkpoint).unwrap_or_default();
                            tx.execute("UPDATE workflow_runs SET status = 'Paused', current_node_id = ?2 WHERE id = ?1", rusqlite::params![run_id.as_str(), checkpoint_json])?;
                            Ok(())
                        });
                        return Ok(());
                    }
                }
                WorkflowNodeType::Condition => {
                    let script = current_node
                        .task_description
                        .clone()
                        .unwrap_or_else(|| "exit 0".to_string());

                    let shell = if cfg!(target_os = "windows") {
                        "powershell"
                    } else {
                        "sh"
                    };
                    let shell_arg = if cfg!(target_os = "windows") {
                        "-Command"
                    } else {
                        "-c"
                    };

                    let output = tokio::process::Command::new(shell)
                        .arg(shell_arg)
                        .arg(&script)
                        .output()
                        .await
                        .map_err(|e| {
                            Trans4mersError::Internal(format!(
                                "Failed to execute condition script: {}",
                                e
                            ))
                        })?;

                    let is_true = output.status.success();

                    self.emit_event(
                        &project_id,
                        DomainEvent::WorkflowNodeCompleted {
                            run_id,
                            node_id: current_node_id,
                        },
                    )
                    .await?;

                    let target_condition = if is_true { "true" } else { "false" };
                    let outgoing_edges: Vec<_> = workflow
                        .edges
                        .iter()
                        .filter(|e| {
                            e.source_node_id == current_node_id
                                && e.condition.as_deref() == Some(target_condition)
                        })
                        .collect();

                    for edge in outgoing_edges {
                        active_nodes.push(edge.target_node_id);
                    }
                    continue;
                }
                WorkflowNodeType::Parallel => {
                    self.emit_event(
                        &project_id,
                        DomainEvent::WorkflowNodeCompleted {
                            run_id,
                            node_id: current_node_id,
                        },
                    )
                    .await?;

                    let outgoing_edges: Vec<_> = workflow
                        .edges
                        .iter()
                        .filter(|e| e.source_node_id == current_node_id)
                        .collect();
                    for edge in outgoing_edges {
                        active_nodes.push(edge.target_node_id);
                    }
                    continue;
                }
            }

            self.emit_event(
                &project_id,
                DomainEvent::WorkflowNodeCompleted {
                    run_id,
                    node_id: current_node_id,
                },
            )
            .await?;

            let outgoing_edges: Vec<_> = workflow
                .edges
                .iter()
                .filter(|e| e.source_node_id == current_node_id)
                .collect();
            for edge in outgoing_edges {
                active_nodes.push(edge.target_node_id);
            }
        }

        self.emit_event(&project_id, DomainEvent::WorkflowRunCompleted { run_id })
            .await?;

        Ok(())
    }

    async fn emit_event(
        &self,
        project_id: &ProjectId,
        event: DomainEvent,
    ) -> Result<(), Trans4mersError> {
        let db = self
            .state
            .get_project_db(project_id)
            .ok_or_else(|| Trans4mersError::Database("Project DB not found".to_string()))?;

        let envelope = db.with_write_tx(|tx| {
            crate::cqrs::commit_event(
                tx,
                event,
                ActorId::new(), // Workflows act as System
            )
        })?;

        let env_arc = std::sync::Arc::new(envelope);
        let bus = self.state.get_event_bus(project_id);
        let _ = bus.publish(env_arc.clone());
        let _ = self.state.global_event_bus.publish(env_arc);

        Ok(())
    }
}
