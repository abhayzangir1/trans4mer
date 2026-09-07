use crate::app_state::AppState;
use crate::memory_engine::MemoryEngine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::Arc;
use tracing::{info, warn};
use trans4mers_domain::config::ModelConfig;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::ids::{ConversationId, MemoryId, ProjectId};
use trans4mers_domain::memory::{
    Memory, MemoryLifecycle, MemoryProvenance, MemoryScope, MemoryTier,
};
use trans4mers_domain::provider::{LlmMessage, LlmRequest};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistilledFact {
    pub summary: String,
    pub importance: f32,
    pub tier: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistillationResult {
    pub facts: Vec<DistilledFact>,
}

pub struct DistillationEngine;

impl DistillationEngine {
    /// Background extraction runs during the housekeeping cycle.
    /// Scans conversations that have messages newer than their last distillation marker,
    /// distills enduring facts, preferences, and knowledge, and stores them in cognitive memory.
    pub async fn run_cycle(
        app_state: &AppState,
        project_id: &ProjectId,
    ) -> Result<usize, Trans4mersError> {
        let db = match app_state.get_project_db(project_id) {
            Some(d) => d,
            None => return Ok(0),
        };

        // 1. Identify conversations that may have un-distilled messages
        let mut candidate_convs: Vec<(String, String)> = Vec::new();
        db.with_read_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT c.id, COALESCE(m.last_distilled_at, '1970-01-01T00:00:00Z')
                 FROM conversations c
                 LEFT JOIN conversation_distillation_markers m ON c.id = m.conversation_id
                 ORDER BY c.updated_at DESC
                 LIMIT 5",
                )
                .map_err(|e| Trans4mersError::Database(e.to_string()))?;

            let rows = stmt
                .query_map([], |row| {
                    let id: String = row.get(0)?;
                    let last_distilled: String = row.get(1)?;
                    Ok((id, last_distilled))
                })
                .map_err(|e| Trans4mersError::Database(e.to_string()))?;

            for r in rows.flatten() {
                candidate_convs.push(r);
            }
            Ok(())
        })?;

        let provider = match app_state.provider_registry.get_default() {
            Ok(p) => p,
            Err(_) => return Ok(0),
        };

        let mut total_facts_stored = 0;
        let memory_engine = MemoryEngine::new(Arc::new(app_state.clone()));

        for (conv_id, last_distilled_str) in candidate_convs {
            // Fetch un-distilled messages
            let mut messages: Vec<(String, String, String, String)> = Vec::new();
            db.with_read_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT id, sender_actor, content, created_at
                     FROM messages
                     WHERE conversation_id = ?1 AND created_at > ?2
                     ORDER BY created_at ASC
                     LIMIT 30",
                    )
                    .map_err(|e| Trans4mersError::Database(e.to_string()))?;

                let rows = stmt
                    .query_map(rusqlite::params![conv_id, last_distilled_str], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                        ))
                    })
                    .map_err(|e| Trans4mersError::Database(e.to_string()))?;

                for r in rows.flatten() {
                    messages.push(r);
                }
                Ok(())
            })?;

            // We require at least 2 messages to form meaningful dialogue
            if messages.len() < 2 {
                continue;
            }

            // Check inactivity: ensure the most recent message was created at least 30 seconds ago
            let last_msg = messages.last().unwrap();
            let last_msg_time = DateTime::parse_from_rfc3339(&last_msg.3)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());

            if Utc::now()
                .signed_duration_since(last_msg_time)
                .num_seconds()
                < 30
            {
                continue;
            }

            // Build dialogue transcript
            let mut transcript = String::new();
            for (_, sender, content, _) in &messages {
                let sender_label = if sender.contains("User") {
                    "User"
                } else if sender.contains("Agent") {
                    "Agent"
                } else {
                    "Participant"
                };
                transcript.push_str(&format!("{}: {}\n", sender_label, content.trim()));
            }

            let schema = serde_json::json!({
                "type": "object",
                "properties": {
                    "facts": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "summary": { "type": "string" },
                                "importance": { "type": "number" },
                                "tier": {
                                    "type": "string",
                                    "enum": ["Working", "Episodic", "Semantic", "Procedural"]
                                }
                            },
                            "required": ["summary", "importance", "tier"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["facts"],
                "additionalProperties": false
            });

            let model_config = ModelConfig {
                provider: provider.name().to_string(),
                temperature: Some(0.1),
                max_output_tokens: Some(1024),
                context_limit: Some(4096),
                ..Default::default()
            };

            let req = LlmRequest {
                messages: vec![
                    LlmMessage {
                        role: "system".to_string(),
                        content: "You are a memory distillation worker. Analyze the dialogue and extract durable facts, user preferences, instructions, or procedural outcomes. Return ONLY JSON.".to_string(),
                    },
                    LlmMessage {
                        role: "user".to_string(),
                        content: format!("Extract salient durable facts from this dialogue:\n\n{}", transcript),
                    }
                ],
                config: model_config.clone(),
                tools: None,
                response_schema: Some(schema),
            };

            let response = match provider.generate(&req).await {
                Ok(res) => res,
                Err(e) => {
                    warn!(conversation_id = %conv_id, error = %e, "Distillation LLM generation failed");
                    continue;
                }
            };

            let parsed: Option<DistillationResult> =
                serde_json::from_str(&response.content).ok().or_else(|| {
                    let text = response.content.trim();
                    let start = text.find('{')?;
                    let end = text.rfind('}')?;
                    serde_json::from_str(&text[start..=end]).ok()
                });

            let facts = match parsed {
                Some(res) => res.facts,
                None => {
                    warn!(conversation_id = %conv_id, "Could not parse distillation JSON from LLM output");
                    continue;
                }
            };

            let now = Utc::now();
            let parsed_conv_id = ConversationId::from_str(&conv_id).ok();

            for fact in facts {
                let tier = MemoryTier::from_str(&fact.tier).unwrap_or(MemoryTier::Semantic);
                let importance = fact.importance.clamp(0.1, 1.0);

                let hash = crate::memory_engine::compute_memory_content_hash(&fact.summary);

                // Deduplicate on write: if active memory with same content hash exists, reinforce it
                let existing = db
                    .with_read_conn(|conn| {
                        trans4mers_storage::repos::memory_repo::find_by_content_hash(
                            conn,
                            &project_id.to_string(),
                            &hash,
                        )
                    })
                    .unwrap_or(None);

                if let Some(existing_mem) = existing {
                    let _ = db.with_write_tx(|conn| {
                        trans4mers_storage::repos::memory_repo::reinforce_memory(
                            conn,
                            &existing_mem.id.to_string(),
                            0.1,
                        )
                    });
                    total_facts_stored += 1;
                    continue;
                }

                let embedding = provider.embed(&fact.summary, &model_config).await.ok();

                let mem = Memory {
                    id: MemoryId::new(),
                    project_id: *project_id,
                    conversation_id: parsed_conv_id,
                    agent_instance_id: None,
                    scope: MemoryScope::Conversation,
                    lifecycle: MemoryLifecycle::Candidate,
                    content: fact.summary,
                    embedding,
                    importance,
                    confidence: 0.85,
                    provenance: MemoryProvenance {
                        source_event_id: None,
                        source_message_id: None,
                        source_agent_id: None,
                        creation_reason: format!("Distilled from conversation {}", conv_id),
                    },
                    depth_level: 1,
                    tier,
                    retrieval_count: 0,
                    last_retrieved_at: None,
                    expires_at: None,
                    valid_from: Some(now),
                    valid_until: None,
                    visibility_overrides: None,
                    created_at: now,
                    updated_at: now,
                    valid_at: Some(now),
                    invalid_at: None,
                    replaced_by: None,
                    content_hash: Some(hash),
                };

                if memory_engine.store_memory(mem).is_ok() {
                    total_facts_stored += 1;
                }
            }

            // Update distillation marker to the timestamp of the last processed message
            let last_ts = &last_msg.3;
            let _ = db.with_write_tx(|conn| {
                conn.execute(
                    "INSERT INTO conversation_distillation_markers (conversation_id, last_distilled_at, created_at)
                     VALUES (?1, ?2, ?3)
                     ON CONFLICT(conversation_id) DO UPDATE SET last_distilled_at = excluded.last_distilled_at",
                    rusqlite::params![conv_id, last_ts, now.to_rfc3339()],
                ).map_err(|e| Trans4mersError::Database(e.to_string()))?;
                Ok(())
            });

            info!(conversation_id = %conv_id, count = messages.len(), "Distilled conversation messages into cognitive memory");
        }

        Ok(total_facts_stored)
    }
}
