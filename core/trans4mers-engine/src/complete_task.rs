use crate::app_state::AppState;
use crate::git_workspace::GitWorkspace;
use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;
use tracing::info;
use trans4mers_domain::actor::Actor;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::event::DomainEvent;
use trans4mers_domain::ids::{
    ActorId, AgentInstanceId, ChannelId, ConversationId, ExecutionId, MessageId,
};
use trans4mers_domain::message::{Message, MessageKind};
use trans4mers_domain::tool::{
    EffectClass, RiskLevel, Tool, ToolManifest, ToolRequest, ToolResult, ToolSource,
};

pub struct CompleteTaskTool {
    manifest: ToolManifest,
    app_state: Arc<AppState>,
}

impl CompleteTaskTool {
    pub fn new(app_state: Arc<AppState>) -> Self {
        Self {
            manifest: ToolManifest {
                name: "complete_task".to_string(),
                description: "Signals completion of the delegated assignment. Commits worktree changes, merges the agent's branch back into main, prunes the worktree directory, marks agent as finished, and delivers the completion report to the delegating parent agent's inbox.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "summary": {
                            "type": "string",
                            "description": "Exhaustive summary of the task findings, changes made, and final result."
                        },
                        "artifacts_produced": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "List of relative paths to files modified or created."
                        }
                    },
                    "required": ["summary"]
                }),
                effect_class: EffectClass::NonIdempotentMutation,
                baseline_risk: RiskLevel::Low,
                required_capabilities: vec![],
                source: ToolSource::Native,
                output_schema: None,
                verification_command: None,
            },
            app_state,
        }
    }
}

#[async_trait]
impl Tool for CompleteTaskTool {
    fn manifest(&self) -> &ToolManifest {
        &self.manifest
    }

    async fn execute(&self, request: &ToolRequest) -> Result<ToolResult, Trans4mersError> {
        let summary = request
            .arguments
            .get("summary")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Trans4mersError::Internal("Missing 'summary' argument".to_string()))?;

        let artifacts: Vec<String> = request
            .arguments
            .get("artifacts_produced")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let agent_id = &request.requesting_agent_id;
        let project_id = &request.project_id;
        let db = self
            .app_state
            .get_project_db(project_id)
            .ok_or_else(|| Trans4mersError::Database("Project DB not found".to_string()))?;

        // 1. Check parent agent and conversation
        let (parent_id_opt, conversation_id_opt) = db
            .with_read_conn(|conn| {
                let mut stmt = conn
                    .prepare("SELECT parent_instance_id FROM agent_instances WHERE id = ?1")
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                let parent_res: Option<String> = stmt
                    .query_row([agent_id.as_str()], |row| row.get(0))
                    .ok()
                    .flatten();

                let mut exec_stmt = conn
                    .prepare("SELECT conversation_id FROM agent_executions WHERE id = ?1")
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                let convo_res: Option<String> = exec_stmt
                    .query_row([request.execution_id.as_str()], |row| row.get(0))
                    .ok();

                Ok((
                    parent_res.and_then(|s| AgentInstanceId::from_str(&s).ok()),
                    convo_res.and_then(|s| ConversationId::from_str(&s).ok()),
                ))
            })
            .map_err(|e| Trans4mersError::Internal(e.to_string()))?;

        // 2. Resolve project base workspace path
        let mut base_workspace_path = None;
        let _ = self.app_state.global_db.with_read_conn(|conn| {
            if let Ok(Some(proj)) =
                trans4mers_storage::repos::project_repo::get_project(conn, project_id)
            {
                base_workspace_path = Some(std::path::PathBuf::from(proj.workspace_path));
            }
            Ok(())
        });

        let base_path = base_workspace_path.ok_or_else(|| {
            Trans4mersError::Internal("Could not resolve project base path".to_string())
        })?;

        let worktree_dir = base_path.join(format!(".trans4mers/worktrees/{}", agent_id));
        let branch_name = format!("agent/{}", agent_id);

        let mut merge_summary = String::new();

        // 3. If agent has a dedicated worktree, commit changes, merge branch, and cleanup
        if worktree_dir.exists() {
            info!(
                "Agent {} has active worktree at {:?}. Finalizing git operations.",
                agent_id, worktree_dir
            );

            // Stage and commit any outstanding changes
            let commit_msg = format!("Agent {} completed task: {}", agent_id, summary);
            let committed = GitWorkspace::commit_worktree_changes(&worktree_dir, &commit_msg)?;
            if committed {
                merge_summary.push_str("Worktree changes committed. ");
            } else {
                merge_summary.push_str("No modified files in worktree. ");
            }

            // Merge back into main
            match GitWorkspace::merge_agent_branch(&base_path, &branch_name) {
                Ok(_) => {
                    merge_summary.push_str("Branch successfully merged to main. ");
                }
                Err(e) => {
                    tracing::warn!("Branch merge skipped or encountered conflict: {}", e);
                    merge_summary.push_str(&format!("Branch merge warning: {}. ", e));
                }
            }

            // Prune and cleanup the isolated worktree directory
            if let Err(e) = GitWorkspace::cleanup_agent_worktree(&base_path, &agent_id.to_string())
            {
                tracing::warn!("Failed to cleanup worktree for agent {}: {}", agent_id, e);
            } else {
                merge_summary.push_str("Worktree directory pruned. ");
            }
        }

        // 4. Mark agent status as Retired and execution Completed in project DB and emit DomainEvents
        let _ =
            db.with_write_tx(|conn| {
                conn.execute(
                    "UPDATE agent_instances SET status = 'Retired', updated_at = ?2 WHERE id = ?1",
                    rusqlite::params![agent_id.as_str(), chrono::Utc::now().to_rfc3339()],
                )?;
                conn.execute(
                "UPDATE agent_executions SET status = 'Completed', updated_at = ?2 WHERE id = ?1",
                rusqlite::params![request.execution_id.as_str(), chrono::Utc::now().to_rfc3339()]
            )?;
                let status_event = DomainEvent::AgentStatusChanged {
                    agent_instance_id: *agent_id,
                    old_status: trans4mers_domain::state::AgentStatus::Active,
                    new_status: trans4mers_domain::state::AgentStatus::Retired,
                };
                crate::cqrs::commit_event(conn, status_event, ActorId::new())?;

                let exec_event = DomainEvent::ExecutionCompleted {
                    execution_id: request.execution_id,
                };
                crate::cqrs::commit_event(conn, exec_event, ActorId::new())
            });

        // 5. Notify the parent agent if one exists
        if let Some(parent_id) = parent_id_opt {
            let convo_id = conversation_id_opt
                .unwrap_or_else(|| ConversationId::from_uuid(*project_id.as_uuid()));
            let return_msg = Message {
                id: MessageId::new(),
                conversation_id: convo_id,
                channel_id: ChannelId::from_uuid(*parent_id.as_uuid()),
                sender: Actor::Agent(*agent_id),
                content: format!(
                    "Sub-agent task completed.\n\nSummary: {}\nArtifacts: {:?}\n\nGit Status: {}",
                    summary, artifacts, merge_summary
                ),
                mentions: vec![],
                message_kind: MessageKind::Chat,
                thread_id: None,
                attachments: vec![],
                requires_approval: false,
                approval_id: None,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            };

            // Queue in parent inbox
            let inbox_event = DomainEvent::InboxMessageQueued {
                message_id: return_msg.id.to_string(),
                recipient_agent_id: parent_id,
                sender_actor_id: format!("Agent:{}", agent_id),
                payload: serde_json::to_string(&return_msg).unwrap_or_default(),
            };

            let _ = db
                .with_write_tx(|conn| crate::cqrs::commit_event(conn, inbox_event, ActorId::new()));

            // Find parent execution to wake it up in the scheduler
            let mut parent_exec_opt: Option<ExecutionId> = None;
            let _ = db.with_read_conn(|conn| {
                if let Ok(id_str) = conn.query_row(
                    "SELECT id FROM agent_executions WHERE agent_instance_id = ?1 AND status != 'Completed' AND status != 'Failed' ORDER BY created_at DESC LIMIT 1",
                    rusqlite::params![parent_id.as_str()],
                    |row| row.get::<_, String>(0)
                ) {
                    parent_exec_opt = ExecutionId::from_str(&id_str).ok();
                }
                Ok(())
            });

            if let Some(p_exec) = parent_exec_opt {
                self.app_state.scheduler.queue(p_exec, *project_id);
            }
        }

        Ok(ToolResult {
            success: true,
            content: format!(
                "Task successfully completed and finalized. {}",
                merge_summary
            ),
            error: None,
            artifacts,
            metadata: std::collections::HashMap::new(),
        })
    }
}
