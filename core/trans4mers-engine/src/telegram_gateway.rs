use crate::app_state::AppState;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use trans4mers_domain::ids::ProjectId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramUpdate {
    pub update_id: i64,
    pub message: Option<TelegramMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramMessage {
    pub message_id: i64,
    pub from: Option<TelegramUser>,
    pub chat: TelegramChat,
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramUser {
    pub id: i64,
    pub first_name: String,
    pub username: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramChat {
    pub id: i64,
    pub r#type: String,
    pub title: Option<String>,
    pub username: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TelegramUpdatesResponse {
    pub ok: bool,
    pub result: Vec<TelegramUpdate>,
}

#[derive(Debug, Clone, Serialize)]
struct SendMessagePayload<'a> {
    pub chat_id: i64,
    pub text: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parse_mode: Option<&'a str>,
}

pub struct TelegramGateway {
    app_state: Arc<AppState>,
    client: reqwest::Client,
}

impl TelegramGateway {
    pub fn new(app_state: Arc<AppState>) -> Self {
        Self {
            app_state,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(45))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Spawns the Telegram long-polling background worker
    pub fn start(self: Arc<Self>, cancel_token: CancellationToken) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            tracing::info!("Telegram Gateway background service initialized");
            let mut offset = 0i64;

            loop {
                tokio::select! {
                    _ = cancel_token.cancelled() => {
                        tracing::info!("Telegram Gateway worker received shutdown signal");
                        break;
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_millis(500)) => {
                        let (enabled, bot_token, allowed_chat_ids, default_project_id) = {
                            let cfg = self.app_state.config.read().await;
                            (
                                cfg.telegram.enabled,
                                cfg.telegram.bot_token.clone(),
                                cfg.telegram.allowed_chat_ids.clone(),
                                cfg.telegram.default_project_id.clone(),
                            )
                        };

                        if !enabled {
                            // Sleep before re-checking config
                            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                            continue;
                        }

                        let token = match bot_token {
                            Some(ref t) if !t.trim().is_empty() => t.trim().to_string(),
                            _ => {
                                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                                continue;
                            }
                        };

                        match self.poll_updates(&token, offset).await {
                            Ok(updates) => {
                                for update in updates {
                                    if update.update_id >= offset {
                                        offset = update.update_id + 1;
                                    }

                                    if let Some(msg) = update.message {
                                        let chat_id = msg.chat.id;
                                        // Strict Fail-Closed authentication: reject if whitelist is empty or chat_id is not authorized
                                        if allowed_chat_ids.is_empty() || !allowed_chat_ids.contains(&chat_id) {
                                            tracing::warn!("Telegram message rejected from unauthorized or unconfigured chat_id: {}", chat_id);
                                            let _ = self.send_text_message(&token, chat_id, "⛔ Access Denied: This Trans4mers node enforces fail-closed authentication. Your chat ID is not on the allowed whitelist.").await;
                                            continue;
                                        }

                                        if let Some(text) = msg.text {
                                            let target_project = default_project_id.clone().unwrap_or_else(|| "default".to_string());
                                            self.handle_incoming_text(&token, chat_id, &text, &target_project).await;
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::error!("Telegram polling error: {}", e);
                                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                            }
                        }
                    }
                }
            }
        })
    }

    async fn poll_updates(
        &self,
        token: &str,
        offset: i64,
    ) -> Result<Vec<TelegramUpdate>, reqwest::Error> {
        let url = format!(
            "https://api.telegram.org/bot{}/getUpdates?offset={}&timeout=30",
            token, offset
        );
        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let body: TelegramUpdatesResponse = resp.json().await?;
        if body.ok { Ok(body.result) } else { Ok(vec![]) }
    }

    pub async fn send_text_message(
        &self,
        token: &str,
        chat_id: i64,
        text: &str,
    ) -> Result<(), reqwest::Error> {
        let url = format!("https://api.telegram.org/bot{}/sendMessage", token);
        let payload = SendMessagePayload {
            chat_id,
            text,
            parse_mode: None,
        };
        let _ = self.client.post(&url).json(&payload).send().await?;
        Ok(())
    }

    async fn handle_incoming_text(&self, token: &str, chat_id: i64, text: &str, project_id: &str) {
        let trimmed = text.trim();
        if trimmed == "/start" {
            let welcome = "🤖 *Trans4mers Autonomous Node*\n\nConnected to your local AI engine.\n\nAvailable commands:\n• `/status` - Check running agents and system health\n• `/ping` - Health check\n• Send any text to prompt the Boss Agent directly.";
            let _ = self.send_text_message(token, chat_id, welcome).await;
            return;
        }

        if trimmed == "/ping" {
            let _ = self
                .send_text_message(token, chat_id, "🏓 Pong! Trans4mers engine online.")
                .await;
            return;
        }

        let pid = if let Ok(parsed) = ProjectId::from_str(project_id) {
            if self.app_state.get_project_db(&parsed).is_some() {
                Some(parsed)
            } else {
                None
            }
        } else {
            None
        };

        let pid = match pid {
            Some(p) => p,
            None => {
                let fallback = self
                    .app_state
                    .global_db
                    .with_read_conn(|conn| {
                        let mut stmt = conn
                            .prepare("SELECT id FROM projects ORDER BY created_at ASC LIMIT 1")?;
                        let mut rows = stmt.query([])?;
                        if let Some(row) = rows.next()? {
                            let id_str: String = row.get(0)?;
                            Ok(ProjectId::from_str(&id_str).ok())
                        } else {
                            Ok(None)
                        }
                    })
                    .unwrap_or(None);

                match fallback {
                    Some(p) => p,
                    None => {
                        let _ = self.send_text_message(token, chat_id, "⚠️ No active project found on this Trans4mers node. Please create or open a project in the desktop app first.").await;
                        return;
                    }
                }
            }
        };

        if trimmed == "/status" {
            let status_msg = format!(
                "📊 Trans4mers Engine Status:\n• Active Project: {}\n• Scheduler Active: true\n• Mode: Autonomous Pair Programming",
                pid
            );
            let _ = self.send_text_message(token, chat_id, &status_msg).await;
            return;
        }

        // Forward user message as a new turn for the agent
        let ack_msg = format!(
            "⚡ Dispatching to Boss Agent in project '{}':\n\"{}\"",
            pid, trimmed
        );
        let _ = self.send_text_message(token, chat_id, &ack_msg).await;

        let event = trans4mers_domain::event::DomainEvent::TelegramMessageReceived {
            chat_id,
            text: trimmed.to_string(),
            project_id: pid.to_string(),
        };

        if let Some(db) = self.app_state.get_project_db(&pid) {
            // 1. Commit TelegramMessageReceived event for event log
            if let Ok(envelope) = db.with_write_tx(|conn| {
                crate::cqrs::commit_event(conn, event, trans4mers_domain::ids::ActorId::new())
            }) {
                let env_arc = Arc::new(envelope);
                let event_bus = self.app_state.get_event_bus(&pid);
                let _ = event_bus.publish(env_arc.clone());
                let _ = self.app_state.global_event_bus.publish(env_arc);
            }

            // 2. Find or spawn sovereign agent for project
            let mut target_agent_opt: Option<trans4mers_domain::ids::AgentInstanceId> = None;
            let _ = db.with_read_conn(|conn| {
                let id_res: Result<String, _> = conn.query_row(
                    "SELECT id FROM agent_instances WHERE project_id = ?1 AND status != 'TERMINATED' ORDER BY created_at ASC LIMIT 1",
                    rusqlite::params![pid.as_str()],
                    |row| row.get(0)
                );
                if let Ok(id_str) = id_res {
                    target_agent_opt = trans4mers_domain::ids::AgentInstanceId::from_str(&id_str).ok();
                }
                Ok(())
            });

            let target_agent = if let Some(a_id) = target_agent_opt {
                a_id
            } else {
                let boss_id = trans4mers_domain::ids::AgentInstanceId::new();
                let spawn_event = trans4mers_domain::event::DomainEvent::AgentSpawned {
                    agent_id: boss_id,
                    project_id: pid,
                    definition_id: "boss".to_string(),
                    parent_id: None,
                };
                let _ = db.with_write_tx(|conn| {
                    crate::cqrs::commit_event(
                        conn,
                        spawn_event,
                        trans4mers_domain::ids::ActorId::new(),
                    )
                });
                boss_id
            };

            // 3. Create genuine chat message so it persists in the project conversation
            let msg = trans4mers_domain::message::Message {
                id: trans4mers_domain::ids::MessageId::new(),
                conversation_id: trans4mers_domain::ids::ConversationId::from_str(project_id)
                    .unwrap_or_else(|_| trans4mers_domain::ids::ConversationId::new()),
                channel_id: trans4mers_domain::ids::ChannelId::from_str("general")
                    .unwrap_or_else(|_| trans4mers_domain::ids::ChannelId::new()),
                sender: trans4mers_domain::actor::Actor::Human {
                    id: trans4mers_domain::ids::ActorId::new(),
                    display_name: format!("Telegram User {}", chat_id),
                },
                content: trimmed.to_string(),
                mentions: vec![],
                message_kind: trans4mers_domain::message::MessageKind::Chat,
                thread_id: None,
                attachments: vec![],
                requires_approval: false,
                approval_id: None,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            };

            let msg_event = trans4mers_domain::event::DomainEvent::MessageSent {
                message: msg.clone(),
            };

            let _ = db.with_write_tx(|conn| {
                crate::cqrs::commit_event(conn, msg_event, trans4mers_domain::ids::ActorId::new())
            });

            // 4. Resolve or create execution for the target agent
            let mut exec_id_opt: Option<trans4mers_domain::ids::ExecutionId> = None;
            let _ = db.with_read_conn(|conn| {
                if let Ok(exec_id_str) = conn.query_row(
                    "SELECT id FROM agent_executions WHERE agent_instance_id = ?1 AND status IN ('Pending', 'Running', 'Checkpointing', 'WaitingForApproval', 'WaitingForMessage')",
                    rusqlite::params![target_agent.as_str()],
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
                let _ = db.with_write_tx(|conn| {
                    conn.execute(
                        "INSERT INTO agent_executions (id, agent_instance_id, conversation_id, status, generation, current_step, max_steps, updated_at) VALUES (?1, ?2, ?3, 'Pending', 0, 0, 100, ?4)",
                        rusqlite::params![new_id.as_str(), target_agent.as_str(), pid.as_str(), chrono::Utc::now().to_rfc3339()]
                    )?;
                    Ok(())
                });
                new_id
            };

            // 5. Queue message into agent inbox and notify scheduler
            let inbox_event = trans4mers_domain::event::DomainEvent::InboxMessageQueued {
                message_id: trans4mers_domain::ids::MessageId::new().to_string(),
                recipient_agent_id: target_agent,
                sender_actor_id: format!("telegram:{}", chat_id),
                payload: serde_json::to_string(&msg).unwrap_or_default(),
            };

            if let Ok(envelope) = db.with_write_tx(|conn| {
                crate::cqrs::commit_event(conn, inbox_event, trans4mers_domain::ids::ActorId::new())
            }) {
                let env_arc = Arc::new(envelope);
                let event_bus = self.app_state.get_event_bus(&pid);
                let _ = event_bus.publish(env_arc.clone());
                let _ = self.app_state.global_event_bus.publish(env_arc);
                self.app_state.scheduler.queue(exec_id, pid);
            }
        }
    }
}
