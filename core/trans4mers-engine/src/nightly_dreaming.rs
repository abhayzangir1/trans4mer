use crate::app_state::AppState;
use chrono::{DateTime, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};
use trans4mers_domain::config::ModelConfig;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::ids::ProjectId;
use trans4mers_domain::provider::{LlmMessage, LlmRequest};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DreamingSummary {
    pub project_id: String,
    pub timestamp: DateTime<Utc>,
    pub conversations_analyzed: usize,
    pub rules_extracted: usize,
    pub summary: String,
}

pub struct NightlyDreamingWorker;

impl NightlyDreamingWorker {
    /// Spawns the background dreaming check loop (evaluates every 1 hour, triggers at 3 AM local or when enabled)
    pub fn spawn_dreaming_loop(app_state: Arc<AppState>) {
        tokio::spawn(async move {
            info!("Nightly Dreaming background loop initialized.");
            let mut ticker = tokio::time::interval(Duration::from_secs(3600)); // Check hourly
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                ticker.tick().await;

                if app_state.shutdown_manager.is_shutting_down() {
                    break;
                }

                // Check global config toggle
                let is_enabled = app_state
                    .config
                    .read()
                    .await
                    .features
                    .nightly_dreaming_enabled;
                if !is_enabled {
                    debug!(
                        "Nightly dreaming is toggled OFF. Skipping consolidation pass to protect user credits."
                    );
                    continue;
                }

                let current_hour = chrono::Local::now().hour();
                // Trigger only during the 3 AM hour window
                if current_hour == 3 {
                    info!(
                        "3 AM hour reached. Running autonomous memory consolidation across all open projects."
                    );
                    for entry in app_state.project_dbs.iter() {
                        let pid = entry.key();
                        if let Err(e) = Self::run_consolidation(&app_state, pid).await {
                            warn!("Dreaming consolidation error for project {}: {}", pid, e);
                        }
                    }
                }
            }
        });
    }

    /// Run immediate memory consolidation for a project (callable via IPC or background worker)
    pub async fn run_consolidation(
        app_state: &AppState,
        project_id: &ProjectId,
    ) -> Result<DreamingSummary, Trans4mersError> {
        info!(
            "Starting memory consolidation / dreaming pass for Project {}",
            project_id
        );

        let pdb = app_state.get_project_db(project_id).ok_or_else(|| {
            Trans4mersError::Internal(format!("Project DB not found: {}", project_id))
        })?;

        // 1. Gather recent messages from the past 24 hours across conversations
        let cutoff = Utc::now() - chrono::Duration::hours(24);
        let recent_messages: Vec<(String, String)> = pdb.with_read_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT sender_actor, content FROM messages
                 WHERE created_at >= ?1 ORDER BY created_at ASC LIMIT 100",
            )?;
            let rows = stmt.query_map([cutoff.to_rfc3339()], |row| {
                let sender: String = row.get(0)?;
                let content: String = row.get(1)?;
                Ok((sender, content))
            })?;
            let mut msgs = Vec::new();
            for r in rows {
                msgs.push(r?);
            }
            Ok(msgs)
        })?;

        if recent_messages.is_empty() {
            info!(
                "No new messages in the last 24h for Project {}. Consolidation complete.",
                project_id
            );
            return Ok(DreamingSummary {
                project_id: project_id.to_string(),
                timestamp: Utc::now(),
                conversations_analyzed: 0,
                rules_extracted: 0,
                summary: "No recent messages found to consolidate.".to_string(),
            });
        }

        // 2. Select model: require local Ollama or explicit user-configured dreaming model to prevent credit leakage
        let features = app_state.config.read().await.features.clone();
        let (provider, model_name) = match features.dreaming_model {
            Some(custom_model) => {
                let prov = app_state
                    .provider_registry
                    .get("ollama")
                    .or_else(|_| app_state.provider_registry.get_default())
                    .map_err(|e| {
                        Trans4mersError::Internal(format!(
                            "No LLM provider available for dreaming consolidation: {}",
                            e
                        ))
                    })?;
                (prov, custom_model)
            }
            None => match app_state.provider_registry.get("ollama") {
                Ok(ollama) => (ollama, "qwen2.5-coder:3b".to_string()),
                Err(_) => {
                    warn!(
                        "Nightly dreaming: Local Ollama provider is offline and no custom dreaming model is set. Aborting consolidation pass to protect user credits."
                    );
                    return Ok(DreamingSummary {
                            project_id: project_id.to_string(),
                            timestamp: Utc::now(),
                            conversations_analyzed: 0,
                            rules_extracted: 0,
                            summary: "Consolidation pass skipped: local Ollama provider is offline. Cloud providers were not engaged to protect API credits.".to_string(),
                        });
                }
            },
        };

        let model_config = ModelConfig {
            provider: provider.name().to_string(),
            model: model_name,
            temperature: Some(0.3),
            max_output_tokens: Some(1024),
            timeout_secs: Some(180),
            ..Default::default()
        };

        let mut conversation_transcript = String::new();
        for (sender, content) in &recent_messages {
            conversation_transcript.push_str(&format!("[{}]: {}\n", sender, content));
        }

        let prompt_text = format!(
            "Analyze the following conversation logs from the past 24 hours of autonomous agent sessions.\n\
            Extract:\n\
            1. Key architectural decisions and constraints established.\n\
            2. Hard bug workarounds, syntax fixes, or verified operational rules.\n\
            3. User preferences or recurring workflows.\n\n\
            Format your response strictly as JSON with the schema:\n\
            {{\n\
              \"summary\": \"Concise overview of what happened\",\n\
              \"learned_rules\": [\"Rule 1\", \"Rule 2\"]\n\
            }}\n\n\
            Conversations:\n{}",
            conversation_transcript
        );

        let request = LlmRequest {
            messages: vec![
                LlmMessage {
                    role: "system".to_string(),
                    content: "You are the Trans4mers Autonomous Memory Distillation Worker. Extract durable facts and rules from daily agent activity.".to_string(),
                },
                LlmMessage {
                    role: "user".to_string(),
                    content: prompt_text,
                },
            ],
            config: model_config,
            tools: None,
            response_schema: None,
        };

        let response = provider.generate(&request).await?;
        let parsed: serde_json::Value =
            serde_json::from_str(&response.content).unwrap_or_else(|_| {
                serde_json::json!({
                    "summary": response.content,
                    "learned_rules": []
                })
            });

        let summary_str = parsed
            .get("summary")
            .and_then(|s| s.as_str())
            .unwrap_or("Consolidation finished")
            .to_string();
        let mut rules_count = 0;

        if let Some(rules) = parsed.get("learned_rules").and_then(|r| r.as_array()) {
            for r in rules {
                if let Some(rule_text) = r.as_str() {
                    let rule_id = format!("rule_{}", uuid::Uuid::new_v4().simple());
                    if let Some(pdb) = app_state.get_project_db(project_id) {
                        let _ = pdb.with_write_tx(|conn| {
                            conn.execute(
                                "INSERT INTO learned_rules (id, project_id, rule_text, confidence, metadata, created_at)
                                 VALUES (?1, ?2, ?3, 'high', '{}', ?4)",
                                rusqlite::params![rule_id, project_id.to_string(), rule_text, Utc::now().to_rfc3339()],
                            ).map_err(|e| Trans4mersError::Database(e.to_string()))?;
                            Ok(())
                        });
                        rules_count += 1;
                    }
                }
            }
        }

        info!(
            "Nightly dreaming complete for Project {}: extracted {} rules.",
            project_id, rules_count
        );

        Ok(DreamingSummary {
            project_id: project_id.to_string(),
            timestamp: Utc::now(),
            conversations_analyzed: recent_messages.len(),
            rules_extracted: rules_count,
            summary: summary_str,
        })
    }
}
