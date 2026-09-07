use tauri::State;
use trans4mers_domain::actor::Actor;
use trans4mers_domain::ids::{ChannelId, ConversationId, ExecutionId, MessageId, ProjectId};
use trans4mers_domain::message::{Mention, Message};
use trans4mers_engine::app_state::AppState;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum MentionInput {
    Structured(Mention),
    Raw(String),
}

#[tauri::command]
pub async fn send_message(
    project_id: Option<String>,
    conversation_id: String,
    channel_id: String,
    content: String,
    mentions: Option<Vec<MentionInput>>,
    state: State<'_, AppState>,
) -> Result<Message, String> {
    let proj_id_str = project_id.unwrap_or_else(|| conversation_id.clone());
    let proj_id = ProjectId::from_str(&proj_id_str).map_err(|e| e.to_string())?;
    let convo_id = ConversationId::from_str(&conversation_id).map_err(|e| e.to_string())?;

    let project_db = state
        .get_project_db(&proj_id)
        .ok_or_else(|| "Project DB not found".to_string())?;

    let mention_targets =
        trans4mers_engine::mention_router::MentionRouter::extract_mention_targets(
            &state.global_db,
            &project_db,
            &proj_id,
            &content,
        )
        .unwrap_or_default();

    let mut recipients: Vec<trans4mers_domain::ids::AgentInstanceId> = Vec::new();
    let mut all_mentions: Vec<Mention> = Vec::new();
    if let Some(inputs) = mentions {
        for input in inputs {
            if let MentionInput::Structured(m) = input {
                all_mentions.push(m);
            }
        }
    }

    for m in &all_mentions {
        if !recipients.contains(&m.agent_instance_id) {
            recipients.push(m.agent_instance_id);
        }
    }

    for target in mention_targets {
        let agent_id = if let Some(id) = target.agent_instance_id {
            id
        } else {
            // Auto-spawn an instance of the mentioned agent definition
            let new_id = trans4mers_domain::ids::AgentInstanceId::new();
            let spawn_event = trans4mers_domain::event::DomainEvent::AgentSpawned {
                agent_id: new_id,
                project_id: proj_id,
                definition_id: target.definition_id.clone(),
                parent_id: None,
            };
            let _ = project_db.with_write_tx(|conn| {
                trans4mers_engine::cqrs::commit_event(conn, spawn_event, state.human_actor_id)
            });
            new_id
        };

        if !recipients.contains(&agent_id) {
            recipients.push(agent_id);
            all_mentions.push(Mention {
                agent_instance_id: agent_id,
                display_name: target.definition_id,
                start_offset: 0,
                end_offset: 0,
            });
        }
    }

    // If no explicit mentions, check if sending in a DM or in a shared blackboard channel
    if recipients.is_empty() {
        // 1. Direct message (channel_id corresponds to an agent instance)
        if let Ok(target_agent_id) = trans4mers_domain::ids::AgentInstanceId::from_str(&channel_id)
        {
            let is_agent = project_db
                .with_read_conn(|conn| {
                    let count: Result<i64, _> = conn.query_row(
                        "SELECT COUNT(1) FROM agent_instances WHERE id = ?1",
                        rusqlite::params![target_agent_id.as_str()],
                        |row| row.get(0),
                    );
                    Ok(count.unwrap_or(0) > 0)
                })
                .unwrap_or(false);
            if is_agent {
                recipients.push(target_agent_id);
            }
        }

        // 2. Shared channel fallback -> Route to active sovereign agent in the project
        if recipients.is_empty() {
            let mut active_agent_id_opt: Option<trans4mers_domain::ids::AgentInstanceId> = None;
            let _ = project_db.with_read_conn(|conn| {
                let id_res: Result<String, _> = conn.query_row(
                    "SELECT id FROM agent_instances WHERE project_id = ?1 AND status != 'TERMINATED' ORDER BY created_at ASC LIMIT 1",
                    rusqlite::params![proj_id.as_str()],
                    |row| row.get(0)
                );
                if let Ok(id_str) = id_res {
                    active_agent_id_opt = trans4mers_domain::ids::AgentInstanceId::from_str(&id_str).ok();
                }
                Ok(())
            });

            if let Some(agent_id) = active_agent_id_opt {
                recipients.push(agent_id);
            } else {
                // Auto-spawn the sovereign boss agent for this workspace
                let boss_id = trans4mers_domain::ids::AgentInstanceId::new();
                let spawn_event = trans4mers_domain::event::DomainEvent::AgentSpawned {
                    agent_id: boss_id,
                    project_id: proj_id,
                    definition_id: trans4mers_domain::constants::DEFAULT_ORCHESTRATOR_ROLE
                        .to_string(),
                    parent_id: None,
                };
                let envelope = project_db.with_write_tx(|conn| {
                    trans4mers_engine::cqrs::commit_event(conn, spawn_event, state.human_actor_id)
                })?;
                let env_arc = std::sync::Arc::new(envelope);
                let _ = state.get_event_bus(&proj_id).publish(env_arc.clone());
                let _ = state.global_event_bus.publish(env_arc);
                recipients.push(boss_id);
            }
        }
    }

    let msg = Message {
        id: MessageId::new(),
        conversation_id: convo_id,
        channel_id: ChannelId::from_str(&channel_id).map_err(|e| e.to_string())?,
        sender: Actor::Human {
            id: state.human_actor_id,
            display_name: "Human".to_string(),
        },
        content: content.clone(),
        mentions: all_mentions,
        message_kind: trans4mers_domain::message::MessageKind::Chat,
        thread_id: None,
        attachments: vec![],
        requires_approval: false,
        approval_id: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let event = trans4mers_domain::event::DomainEvent::MessageSent {
        message: msg.clone(),
    };

    let envelope = project_db.with_write_tx(|conn| {
        trans4mers_engine::cqrs::commit_event(conn, event, state.human_actor_id)
    })?;

    let env_arc = std::sync::Arc::new(envelope);
    let bus = state.get_event_bus(&proj_id);
    let _ = bus.publish(env_arc.clone());
    let _ = state.global_event_bus.publish(env_arc);

    for recipient in recipients {
        let mut exec_id_opt: Option<ExecutionId> = None;
        let _ = project_db.with_read_conn(|conn| {
            if let Ok(exec_id_str) = conn.query_row(
                "SELECT id FROM agent_executions WHERE agent_instance_id = ?1 AND conversation_id = ?2 AND status IN ('Pending', 'Running', 'Checkpointing', 'WaitingForApproval', 'WaitingForMessage')",
                rusqlite::params![recipient.as_str(), convo_id.as_str()],
                |row| row.get::<_, String>(0)
            ) {
                exec_id_opt = ExecutionId::from_str(&exec_id_str).ok();
            }
            Ok(())
        });

        let exec_id = if let Some(id) = exec_id_opt {
            id
        } else {
            let new_id = ExecutionId::new();
            let _ = project_db.with_write_tx(|conn| {
                conn.execute(
                    "INSERT INTO agent_executions (id, agent_instance_id, conversation_id, status, generation, current_step, max_steps, updated_at) VALUES (?1, ?2, ?3, 'Pending', 0, 0, 25, ?4)",
                    rusqlite::params![new_id.as_str(), recipient.as_str(), convo_id.as_str(), chrono::Utc::now().to_rfc3339()]
                )?;
                Ok(())
            });
            new_id
        };

        let inbox_event = trans4mers_domain::event::DomainEvent::InboxMessageQueued {
            message_id: MessageId::new().to_string(),
            recipient_agent_id: recipient,
            sender_actor_id: msg.sender.to_string(),
            payload: serde_json::to_string(&msg).unwrap_or_default(),
        };
        let envelope = project_db.with_write_tx(|conn| {
            trans4mers_engine::cqrs::commit_event(conn, inbox_event, state.human_actor_id)
        })?;

        let env_arc = std::sync::Arc::new(envelope);
        let bus = state.get_event_bus(&proj_id);
        let _ = bus.publish(env_arc.clone());
        let _ = state.global_event_bus.publish(env_arc);

        state.scheduler.queue(exec_id, proj_id);
    }

    Ok(msg)
}

#[tauri::command]
pub async fn get_messages(
    project_id: Option<String>,
    conversation_id: String,
    channel_id: String,
    limit: Option<u32>,
    offset: Option<u32>,
    state: State<'_, AppState>,
) -> Result<Vec<Message>, String> {
    let conv_id_str = if conversation_id.trim().is_empty() {
        project_id.clone().unwrap_or_default()
    } else {
        conversation_id
    };
    let proj_id_str = project_id.unwrap_or_else(|| conv_id_str.clone());
    let proj_id = ProjectId::from_str(&proj_id_str).map_err(|e| e.to_string())?;
    let db = state
        .get_project_db(&proj_id)
        .ok_or_else(|| "Project DB not found".to_string())?;
    let msgs = db.with_read_conn(|conn| {
        trans4mers_storage::repos::message_repo::list_messages(
            conn,
            &conv_id_str,
            &channel_id,
            limit.unwrap_or(50),
            offset.unwrap_or(0),
        )
    })?;
    Ok(msgs)
}
