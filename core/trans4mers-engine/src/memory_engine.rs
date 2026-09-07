use crate::app_state::AppState;
use rusqlite::params;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::ids::{AgentInstanceId, ConversationId, ProjectId};
use trans4mers_domain::learning::{Confidence, LearnedRule, RuleMetadata};
use trans4mers_domain::memory::{Memory, MemoryLifecycle, MemoryProvenance, MemoryScope};
use trans4mers_storage::vector_store::SqliteVecStore;

pub fn compute_memory_content_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.trim().to_lowercase().as_bytes());
    format!("{:x}", hasher.finalize())
}

pub struct MemoryEngine {
    app_state: Arc<AppState>,
}

#[derive(Debug, Clone, Default)]
pub struct MemoryRetrievalOptions<'a> {
    pub conversation_id: Option<&'a ConversationId>,
    pub query_text: Option<&'a str>,
    pub query_embedding: Option<&'a [f32]>,
    pub limit: u32,
    pub temporal: bool,
}

impl MemoryEngine {
    pub fn new(app_state: Arc<AppState>) -> Self {
        Self { app_state }
    }

    /// Stores a new semantic memory into the project database.
    pub fn store_memory(
        &self,
        memory: Memory,
    ) -> Result<trans4mers_domain::ids::MemoryId, Trans4mersError> {
        let db = self
            .app_state
            .get_project_db(&memory.project_id)
            .ok_or_else(|| Trans4mersError::Database("Project database not found".to_string()))?;

        let id = memory.id;

        db.with_write_tx(|tx| {
            let expires_at_str = memory.expires_at.map(|d| d.to_rfc3339());
            let vis_overrides_str = memory
                .visibility_overrides
                .map(|v| serde_json::to_string(&v).unwrap_or_default());
            let provenance_str = serde_json::to_string(&memory.provenance).unwrap_or_default();

            let last_retrieved_str = memory.last_retrieved_at.map(|d| d.to_rfc3339());
            let embedding_blob = memory
                .embedding
                .as_ref()
                .map(|e| trans4mers_storage::embedding_to_blob(e));

            let content_hash = memory
                .content_hash
                .clone()
                .unwrap_or_else(|| compute_memory_content_hash(&memory.content));
            let valid_at_str = memory
                .valid_at
                .or(memory.valid_from)
                .unwrap_or_else(chrono::Utc::now)
                .to_rfc3339();
            let invalid_at_str = memory
                .invalid_at
                .or(memory.valid_until)
                .map(|d| d.to_rfc3339());

            tx.execute(
                "INSERT INTO project_memories (
                    id, project_id, conversation_id, agent_instance_id, scope, lifecycle,
                    content, importance, confidence, provenance, depth_level, tier, retrieval_count,
                    last_retrieved_at, expires_at, visibility_overrides, created_at, updated_at, embedding,
                    valid_at, invalid_at, replaced_by, content_hash
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)",
                params![
                    id.to_string(),
                    memory.project_id.to_string(),
                    memory.conversation_id.map(|c| c.to_string()),
                    memory.agent_instance_id.map(|a| a.to_string()),
                    memory.scope.to_string(),
                    memory.lifecycle.to_string(),
                    memory.content,
                    memory.importance,
                    memory.confidence,
                    provenance_str,
                    memory.depth_level,
                    memory.tier.to_string(),
                    memory.retrieval_count,
                    last_retrieved_str,
                    expires_at_str,
                    vis_overrides_str,
                    memory.created_at.to_rfc3339(),
                    memory.updated_at.to_rfc3339(),
                    embedding_blob,
                    valid_at_str,
                    invalid_at_str,
                    memory.replaced_by,
                    content_hash,
                ],
            ).map_err(|e| Trans4mersError::Database(e.to_string()))?;

            // If embedding is present, also insert into vec_project_memories
            if let Some(ref embedding) = memory.embedding {
                let store = SqliteVecStore::new();
                let _ = store.upsert_embedding(tx, "vec_project_memories", &id.to_string(), embedding);
            }

            Ok(())
        })?;

        Ok(id)
    }

    /// Retrieve memories visible to an agent, using scope rules and hybrid search (FTS5 BM25 keyword + optional vector semantic search) fused via RRF (k=60).
    /// If no query or embedding is provided, falls back to recency and importance. Excludes invalidated memories by default.
    pub fn retrieve_memories(
        &self,
        project_id: &ProjectId,
        agent_id: &AgentInstanceId,
        conversation_id: Option<&ConversationId>,
        query_text: Option<&str>,
        query_embedding: Option<&[f32]>,
        limit: u32,
    ) -> Result<Vec<Memory>, Trans4mersError> {
        self.retrieve_memories_opts(
            project_id,
            agent_id,
            MemoryRetrievalOptions {
                conversation_id,
                query_text,
                query_embedding,
                limit,
                temporal: false,
            },
        )
    }

    /// Retrieve memories with optional temporal query mode (when temporal is true, returns both active and invalidated memories with validity windows).
    pub fn retrieve_memories_opts(
        &self,
        project_id: &ProjectId,
        agent_id: &AgentInstanceId,
        opts: MemoryRetrievalOptions<'_>,
    ) -> Result<Vec<Memory>, Trans4mersError> {
        let db = self
            .app_state
            .get_project_db(project_id)
            .ok_or_else(|| Trans4mersError::Database("Project database not found".to_string()))?;

        let results = db.with_read_conn(|conn| {
            let conversation_str = opts
                .conversation_id
                .map(|c| c.to_string())
                .unwrap_or_default();
            let agent_str = agent_id.to_string();
            let proj_str = project_id.to_string();

            let temporal_filter = if opts.temporal {
                ""
            } else {
                "AND (m.invalid_at IS NULL OR datetime(m.invalid_at) > datetime('now')) AND (m.valid_until IS NULL OR datetime(m.valid_until) > datetime('now'))"
            };

            let mut dense_ranked: Vec<Memory> = Vec::new();
            let mut sparse_ranked: Vec<Memory> = Vec::new();

            // 1. Dense retrieval via sqlite-vec (if embedding provided)
            if let Some(embedding) = opts.query_embedding {
                let embedding_bytes: Vec<u8> = embedding
                    .iter()
                    .flat_map(|&f| f.to_le_bytes().to_vec())
                    .collect();

                let query = format!(
                    "
                    SELECT m.id, m.project_id, m.conversation_id, m.agent_instance_id,
                           m.scope, m.lifecycle, m.content, m.importance, m.confidence,
                           m.provenance, m.depth_level, m.tier, m.retrieval_count, m.last_retrieved_at,
                           m.expires_at, m.visibility_overrides, m.created_at, m.updated_at,
                           m.valid_at, m.invalid_at, m.replaced_by, m.content_hash
                    FROM vec_project_memories v
                    INNER JOIN vec_id_map map ON v.rowid = map.rowid
                    INNER JOIN project_memories m ON map.uuid = m.id
                    WHERE m.project_id = ?1
                      AND (m.scope = 'Global' OR m.scope = 'Project'
                           OR (m.scope = 'Conversation' AND m.conversation_id = ?2)
                           OR (m.scope = 'Agent' AND m.agent_instance_id = ?3)
                           OR (m.scope = 'Artifact' AND m.agent_instance_id = ?3)
                           OR (m.scope = 'Ephemeral' AND m.agent_instance_id = ?3 AND (m.expires_at IS NULL OR datetime(m.expires_at) > datetime('now'))))
                      AND m.lifecycle = 'Persisted'
                      {}
                    ORDER BY vec_distance_L2(v.embedding, ?4) ASC
                    LIMIT ?5
                ",
                    temporal_filter
                );

                if let Ok(mut stmt) = conn.prepare(&query)
                    && let Ok(rows) = stmt.query_map(
                        params![
                            proj_str,
                            conversation_str,
                            agent_str,
                            embedding_bytes,
                            opts.limit * 2
                        ],
                        Self::map_memory_row,
                    )
                {
                    for row in rows.flatten() {
                        dense_ranked.push(row);
                    }
                }
            }

            // 2. Sparse retrieval via SQLite FTS5 BM25 (if query_text provided)
            if let Some(q) = opts.query_text {
                let sanitized = trans4mers_storage::fts5::sanitize_fts5_prefix_query(q);
                if !sanitized.is_empty() {
                    let fts_query = format!(
                        "
                        SELECT m.id, m.project_id, m.conversation_id, m.agent_instance_id,
                               m.scope, m.lifecycle, m.content, m.importance, m.confidence,
                               m.provenance, m.depth_level, m.tier, m.retrieval_count, m.last_retrieved_at,
                               m.expires_at, m.visibility_overrides, m.created_at, m.updated_at,
                               m.valid_at, m.invalid_at, m.replaced_by, m.content_hash
                        FROM project_memories m
                        JOIN project_memories_fts ON project_memories_fts.rowid = m.rowid
                        WHERE project_memories_fts MATCH ?1
                          AND m.project_id = ?2
                          AND (m.scope = 'Global' OR m.scope = 'Project'
                               OR (m.scope = 'Conversation' AND m.conversation_id = ?3)
                               OR (m.scope = 'Agent' AND m.agent_instance_id = ?4)
                               OR (m.scope = 'Artifact' AND m.agent_instance_id = ?4)
                               OR (m.scope = 'Ephemeral' AND m.agent_instance_id = ?4 AND (m.expires_at IS NULL OR datetime(m.expires_at) > datetime('now'))))
                          AND m.lifecycle = 'Persisted'
                          {}
                        ORDER BY bm25(project_memories_fts) ASC
                        LIMIT ?5
                    ",
                        temporal_filter
                    );

                    if let Ok(mut stmt) = conn.prepare(&fts_query)
                        && let Ok(rows) = stmt.query_map(
                            params![
                                sanitized,
                                proj_str,
                                conversation_str,
                                agent_str,
                                opts.limit * 2
                            ],
                            Self::map_memory_row,
                        )
                    {
                        for row in rows.flatten() {
                            sparse_ranked.push(row);
                        }
                    }
                }
            }

            // 3. Fusion or Fallback
            let mut final_results: Vec<Memory> = Vec::new();

            if !dense_ranked.is_empty() && !sparse_ranked.is_empty() {
                // RRF Fusion
                let mut scores: HashMap<String, f32> = HashMap::new();
                let mut memory_map: HashMap<String, Memory> = HashMap::new();

                for (rank, mem) in dense_ranked.into_iter().enumerate() {
                    let id_str = mem.id.to_string();
                    *scores.entry(id_str.clone()).or_insert(0.0) += 1.0 / (60.0 + rank as f32);
                    memory_map.insert(id_str, mem);
                }

                for (rank, mem) in sparse_ranked.into_iter().enumerate() {
                    let id_str = mem.id.to_string();
                    *scores.entry(id_str.clone()).or_insert(0.0) += 1.0 / (60.0 + rank as f32);
                    memory_map.insert(id_str, mem);
                }

                let mut sorted: Vec<(String, f32)> = scores.into_iter().collect();
                sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

                for (id, _) in sorted.into_iter().take(opts.limit as usize) {
                    if let Some(mem) = memory_map.remove(&id) {
                        final_results.push(mem);
                    }
                }
            } else if !sparse_ranked.is_empty() {
                final_results = sparse_ranked
                    .into_iter()
                    .take(opts.limit as usize)
                    .collect();
            } else if !dense_ranked.is_empty() {
                final_results = dense_ranked
                    .into_iter()
                    .take(opts.limit as usize)
                    .collect();
            } else {
                // Default fallback: order by importance DESC, created_at DESC
                let fallback_query = format!(
                    "
                    SELECT m.id, m.project_id, m.conversation_id, m.agent_instance_id,
                           m.scope, m.lifecycle, m.content, m.importance, m.confidence,
                           m.provenance, m.depth_level, m.tier, m.retrieval_count, m.last_retrieved_at,
                           m.expires_at, m.visibility_overrides, m.created_at, m.updated_at,
                           m.valid_at, m.invalid_at, m.replaced_by, m.content_hash
                    FROM project_memories m
                    WHERE m.project_id = ?1
                      AND (m.scope = 'Global' OR m.scope = 'Project'
                           OR (m.scope = 'Conversation' AND m.conversation_id = ?2)
                           OR (m.scope = 'Agent' AND m.agent_instance_id = ?3)
                           OR (m.scope = 'Artifact' AND m.agent_instance_id = ?3)
                           OR (m.scope = 'Ephemeral' AND m.agent_instance_id = ?3 AND (m.expires_at IS NULL OR datetime(m.expires_at) > datetime('now'))))
                      AND m.lifecycle = 'Persisted'
                      {}
                    ORDER BY m.importance DESC, m.created_at DESC
                    LIMIT ?4
                ",
                    temporal_filter
                );

                let mut stmt = conn
                    .prepare(&fallback_query)
                    .map_err(|e| Trans4mersError::Database(e.to_string()))?;
                let rows = stmt.query_map(
                    params![proj_str, conversation_str, agent_str, opts.limit],
                    Self::map_memory_row,
                )?;

                for row in rows {
                    final_results.push(row.map_err(|e| Trans4mersError::Database(e.to_string()))?);
                }
            }

            Ok(final_results)
        })?;

        // Increment retrieval count and touch last_retrieved_at for returned memories
        if !results.is_empty() {
            let id_list: Vec<String> = results.iter().map(|m| m.id.to_string()).collect();
            _ = db.with_write_tx(|tx| {
                let now = chrono::Utc::now().to_rfc3339();
                for mem_id in id_list {
                    _ = tx.execute(
                        "UPDATE project_memories SET retrieval_count = retrieval_count + 1, last_retrieved_at = ?1, updated_at = ?1 WHERE id = ?2",
                        params![now, mem_id],
                    );
                }
                Ok(())
            });
        }

        Ok(results)
    }

    fn map_memory_row(row: &rusqlite::Row) -> Result<Memory, rusqlite::Error> {
        let id_str: String = row.get(0)?;
        let proj_id_str: String = row.get(1)?;
        let conv_id_str: Option<String> = row.get(2)?;
        let agent_id_str: Option<String> = row.get(3)?;
        let scope_str: String = row.get(4)?;
        let lifecycle_str: String = row.get(5)?;
        let content: String = row.get(6)?;
        let importance: f64 = row.get(7)?;
        let confidence: f64 = row.get(8)?;
        let provenance_str: String = row.get(9)?;
        let depth_level: u32 = row.get(10)?;
        let tier_str: String = row.get(11)?;
        let retrieval_count: u32 = row.get(12)?;
        let last_retrieved_str: Option<String> = row.get(13)?;
        let expires_at_str: Option<String> = row.get(14)?;
        let vis_overrides_str: Option<String> = row.get(15)?;
        let created_at_str: String = row.get(16)?;
        let updated_at_str: String = row.get(17)?;
        let valid_at_str: Option<String> = row.get(18).ok().flatten();
        let invalid_at_str: Option<String> = row.get(19).ok().flatten();
        let replaced_by: Option<String> = row.get(20).ok().flatten();
        let content_hash: Option<String> = row.get(21).ok().flatten();

        let mem_id = std::str::FromStr::from_str(&id_str).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?;
        let prj_id = std::str::FromStr::from_str(&proj_id_str).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e))
        })?;
        let conv_id = conv_id_str.and_then(|s| std::str::FromStr::from_str(&s).ok());
        let agent_id = agent_id_str.and_then(|s| std::str::FromStr::from_str(&s).ok());

        Ok(Memory {
            id: mem_id,
            project_id: prj_id,
            conversation_id: conv_id,
            agent_instance_id: agent_id,
            scope: std::str::FromStr::from_str(&scope_str).unwrap_or(MemoryScope::Project),
            lifecycle: std::str::FromStr::from_str(&lifecycle_str)
                .unwrap_or(MemoryLifecycle::Persisted),
            content,
            embedding: None,
            importance: importance as f32,
            confidence: confidence as f32,
            provenance: serde_json::from_str(&provenance_str).unwrap_or(MemoryProvenance {
                source_event_id: None,
                source_message_id: None,
                source_agent_id: None,
                creation_reason: "".into(),
            }),
            depth_level,
            tier: std::str::FromStr::from_str(&tier_str)
                .unwrap_or(trans4mers_domain::memory::MemoryTier::Working),
            retrieval_count,
            last_retrieved_at: last_retrieved_str
                .as_ref()
                .map(|s| trans4mers_storage::datetime_util::parse_db_datetime(s)),
            expires_at: expires_at_str
                .as_ref()
                .map(|s| trans4mers_storage::datetime_util::parse_db_datetime(s)),
            valid_from: valid_at_str
                .as_ref()
                .map(|s| trans4mers_storage::datetime_util::parse_db_datetime(s)),
            valid_until: invalid_at_str
                .as_ref()
                .map(|s| trans4mers_storage::datetime_util::parse_db_datetime(s)),
            valid_at: valid_at_str
                .as_ref()
                .map(|s| trans4mers_storage::datetime_util::parse_db_datetime(s)),
            invalid_at: invalid_at_str
                .as_ref()
                .map(|s| trans4mers_storage::datetime_util::parse_db_datetime(s)),
            replaced_by,
            content_hash,
            visibility_overrides: vis_overrides_str
                .map(|s| serde_json::from_str(&s).unwrap_or_default()),
            created_at: trans4mers_storage::datetime_util::parse_db_datetime(&created_at_str),
            updated_at: trans4mers_storage::datetime_util::parse_db_datetime(&updated_at_str),
        })
    }

    fn map_rule_row(row: &rusqlite::Row) -> Result<LearnedRule, rusqlite::Error> {
        let id_str: String = row.get(0)?;
        let proj_id_str: Option<String> = row.get(1)?;
        let rule_text: String = row.get(2)?;
        let confidence_str: String = row.get(3)?;
        let metadata_str: String = row.get(4)?;
        let created_at: String = row.get(5)?;
        let embedding_blob: Option<Vec<u8>> = row.get(6).ok();
        let embedding = embedding_blob.map(|b| trans4mers_storage::blob_to_embedding(&b));

        let confidence = std::str::FromStr::from_str(&confidence_str).unwrap_or(Confidence::Medium);
        let metadata = serde_json::from_str(&metadata_str).unwrap_or(RuleMetadata {
            language: None,
            framework: None,
            file_pattern: None,
            source_agent_id: None,
            source_execution_id: None,
        });

        Ok(LearnedRule {
            id: std::str::FromStr::from_str(&id_str)
                .unwrap_or_else(|_| trans4mers_domain::ids::LearnedRuleId::new()),
            project_id: proj_id_str.and_then(|id| std::str::FromStr::from_str(&id).ok()),
            rule_text,
            confidence,
            metadata,
            embedding,
            created_at: trans4mers_storage::datetime_util::parse_db_datetime(&created_at),
        })
    }

    /// Retrieves relevant learned rules via hybrid search (FTS5 BM25 keyword + optional vector semantic search) fused via RRF (k=60).
    pub fn retrieve_relevant_rules(
        &self,
        project_id: &ProjectId,
        _agent_id: &AgentInstanceId,
        query_text: Option<&str>,
        query_embedding: Option<&[f32]>,
        limit: u32,
    ) -> Result<Vec<LearnedRule>, Trans4mersError> {
        let db = self
            .app_state
            .get_project_db(project_id)
            .ok_or_else(|| Trans4mersError::Database("Project database not found".to_string()))?;

        db.with_read_conn(|conn| {
            let proj_str = project_id.to_string();
            let mut dense_ranked: Vec<LearnedRule> = Vec::new();
            let mut sparse_ranked: Vec<LearnedRule> = Vec::new();

            // 1. Dense retrieval via sqlite-vec (if embedding provided)
            if let Some(embedding) = query_embedding {
                let embedding_bytes: Vec<u8> = embedding
                    .iter()
                    .flat_map(|&f| f.to_le_bytes().to_vec())
                    .collect();

                let query = "
                    SELECT r.id, r.project_id, r.rule_text, r.confidence, r.metadata, r.created_at, r.embedding
                    FROM vec_learned_rules v
                    INNER JOIN vec_id_map map ON v.rowid = map.rowid
                    INNER JOIN learned_rules r ON map.uuid = r.id
                    WHERE r.project_id = ?1 OR r.project_id IS NULL
                    ORDER BY vec_distance_L2(v.embedding, ?2) ASC
                    LIMIT ?3
                ";

                if let Ok(mut stmt) = conn.prepare(query)
                    && let Ok(rows) = stmt.query_map(
                        params![proj_str, embedding_bytes, limit * 2],
                        Self::map_rule_row,
                    ) {
                    for row in rows.flatten() {
                        dense_ranked.push(row);
                    }
                }
            }

            // 2. Sparse retrieval via SQLite FTS5 BM25 (if query_text provided)
            if let Some(q) = query_text {
                let sanitized = trans4mers_storage::fts5::sanitize_fts5_prefix_query(q);
                if !sanitized.is_empty() {
                    let fts_query = "
                        SELECT r.id, r.project_id, r.rule_text, r.confidence, r.metadata, r.created_at, r.embedding
                        FROM learned_rules r
                        JOIN learned_rules_fts ON learned_rules_fts.rowid = r.rowid
                        WHERE learned_rules_fts MATCH ?1
                          AND (r.project_id = ?2 OR r.project_id IS NULL)
                        ORDER BY bm25(learned_rules_fts) ASC
                        LIMIT ?3
                    ";

                    if let Ok(mut stmt) = conn.prepare(fts_query)
                        && let Ok(rows) = stmt.query_map(
                            params![sanitized, proj_str, limit * 2],
                            Self::map_rule_row,
                        ) {
                        for row in rows.flatten() {
                            sparse_ranked.push(row);
                        }
                    }
                }
            }

            // 3. Fusion or Fallback
            let mut final_results: Vec<LearnedRule> = Vec::new();

            if !dense_ranked.is_empty() && !sparse_ranked.is_empty() {
                let dense_ids: Vec<String> = dense_ranked.iter().map(|r| r.id.to_string()).collect();
                let sparse_ids: Vec<String> = sparse_ranked.iter().map(|r| r.id.to_string()).collect();
                let fused_ranks = trans4mers_storage::rrf::reciprocal_rank_fusion(
                    &[&dense_ids, &sparse_ids],
                    60.0,
                );

                let mut rule_map = std::collections::HashMap::new();
                for r in dense_ranked {
                    rule_map.insert(r.id.to_string(), r);
                }
                for r in sparse_ranked {
                    rule_map.insert(r.id.to_string(), r);
                }

                for (id, _score) in fused_ranks.into_iter().take(limit as usize) {
                    if let Some(rule) = rule_map.remove(&id) {
                        final_results.push(rule);
                    }
                }
            } else if !sparse_ranked.is_empty() {
                final_results = sparse_ranked.into_iter().take(limit as usize).collect();
            } else if !dense_ranked.is_empty() {
                final_results = dense_ranked.into_iter().take(limit as usize).collect();
            } else {
                let fallback_query = "
                    SELECT id, project_id, rule_text, confidence, metadata, created_at, embedding
                    FROM learned_rules
                    WHERE project_id = ?1 OR project_id IS NULL
                    ORDER BY created_at DESC
                    LIMIT ?2
                ";
                if let Ok(mut stmt) = conn.prepare(fallback_query)
                    && let Ok(rows) = stmt.query_map(params![proj_str, limit], Self::map_rule_row)
                {
                    for row in rows.flatten() {
                        final_results.push(row);
                    }
                }
            }

            Ok(final_results)
        })
    }

    /// Commits a new learned rule.
    pub fn learn_rule(&self, rule: &LearnedRule, embedding: &[f32]) -> Result<(), Trans4mersError> {
        let proj_id = rule.project_id.as_ref().ok_or_else(|| {
            Trans4mersError::Database("Global rules cannot be saved via Project DB".to_string())
        })?;
        let db = self
            .app_state
            .get_project_db(proj_id)
            .ok_or_else(|| Trans4mersError::Database("Project database not found".to_string()))?;

        db.with_write_tx(|tx| {
            let metadata_json = serde_json::to_string(&rule.metadata).unwrap();
            let confidence_str = rule.confidence.to_string();
            let emb_blob = trans4mers_storage::embedding_to_blob(embedding);

            tx.execute(
                "INSERT INTO learned_rules (id, project_id, rule_text, confidence, metadata, embedding, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    rule.id.to_string(),
                    proj_id.to_string(),
                    rule.rule_text,
                    confidence_str,
                    metadata_json,
                    emb_blob,
                    rule.created_at.to_rfc3339(),
                ]
            ).map_err(|e| Trans4mersError::Database(e.to_string()))?;

            // Must use a raw connection approach within the tx for vector_store
            // tx Derefs to Connection.
            let store = SqliteVecStore::new();
            store.upsert_embedding(tx, "vec_learned_rules", &rule.id.to_string(), embedding)?;

            Ok(())
        })
    }
}
