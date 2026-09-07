use chrono::Utc;
use rusqlite::{Connection, params};
use std::str::FromStr;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::ids::{AgentInstanceId, ConversationId, MemoryId, ProjectId};
use trans4mers_domain::memory::{
    Memory, MemoryLifecycle, MemoryProvenance, MemoryScope, MemoryTier,
};

pub fn insert_project_memory(conn: &Connection, mem: &Memory) -> Result<(), Trans4mersError> {
    let visibility_json = match &mem.visibility_overrides {
        Some(v) => Some(serde_json::to_string(v).map_err(|e| Trans4mersError::Database(e.to_string()))?),
        None => None,
    };
    let last_retrieved_str = mem.last_retrieved_at.map(|d| d.to_rfc3339());
    let embedding_bytes = mem
        .embedding
        .as_ref()
        .map(|e| crate::vector_blob::embedding_to_blob(e));

    let valid_at_str = mem.valid_at.or(mem.valid_from).map(|d| d.to_rfc3339());
    let invalid_at_str = mem.invalid_at.or(mem.valid_until).map(|d| d.to_rfc3339());

    conn.execute(
        "INSERT INTO project_memories (
            id, project_id, conversation_id, agent_instance_id,
            scope, lifecycle, content, importance, confidence,
            provenance, depth_level, tier, retrieval_count, last_retrieved_at,
            expires_at, visibility_overrides, valid_from, valid_until, created_at, updated_at, embedding,
            valid_at, invalid_at, replaced_by, content_hash
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25)",
        params![
            mem.id.as_str(),
            mem.project_id.as_str(),
            mem.conversation_id.as_ref().map(|id| id.as_str()),
            mem.agent_instance_id.as_ref().map(|id| id.as_str()),
            mem.scope.to_string(),
            mem.lifecycle.to_string(),
            mem.content,
            mem.importance,
            mem.confidence,
            serde_json::to_string(&mem.provenance).unwrap_or_default(),
            mem.depth_level,
            mem.tier.to_string(),
            mem.retrieval_count,
            last_retrieved_str,
            mem.expires_at.map(|d| d.to_rfc3339()),
            visibility_json,
            mem.valid_from.map(|d| d.to_rfc3339()),
            mem.valid_until.map(|d| d.to_rfc3339()),
            mem.created_at.to_rfc3339(),
            mem.updated_at.to_rfc3339(),
            embedding_bytes,
            valid_at_str,
            invalid_at_str,
            mem.replaced_by.as_deref(),
            mem.content_hash.as_deref(),
        ],
    ).map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}

pub fn get_project_memory(conn: &Connection, id: &str) -> Result<Option<Memory>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, conversation_id, agent_instance_id, scope, lifecycle,
                content, importance, confidence, provenance, depth_level, tier,
                retrieval_count, last_retrieved_at, expires_at, visibility_overrides,
                created_at, updated_at, valid_from, valid_until, embedding,
                valid_at, invalid_at, replaced_by, content_hash
         FROM project_memories WHERE id = ?1",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut rows = stmt
        .query([id])
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    if let Some(row) = rows
        .next()
        .map_err(|e| Trans4mersError::Database(e.to_string()))?
    {
        Ok(Some(row_to_memory(row)?))
    } else {
        Ok(None)
    }
}

pub fn list_by_tier(
    conn: &Connection,
    project_id: &str,
    tier: MemoryTier,
) -> Result<Vec<Memory>, Trans4mersError> {
    let now_str = Utc::now().to_rfc3339();
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, conversation_id, agent_instance_id, scope, lifecycle,
                content, importance, confidence, provenance, depth_level, tier,
                retrieval_count, last_retrieved_at, expires_at, visibility_overrides,
                created_at, updated_at, valid_from, valid_until, embedding,
                valid_at, invalid_at, replaced_by, content_hash
         FROM project_memories
         WHERE project_id = ?1 AND tier = ?2
           AND (valid_from IS NULL OR valid_from <= ?3)
           AND (invalid_at IS NULL OR invalid_at > ?3)
           AND (valid_until IS NULL OR valid_until > ?3)
           AND (expires_at IS NULL OR expires_at > datetime('now'))
         ORDER BY importance DESC, created_at DESC",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let rows = stmt
        .query_map(
            params![project_id, tier.to_string(), now_str],
            row_to_memory,
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut results = Vec::new();
    for r in rows {
        results.push(r.map_err(|e| Trans4mersError::Database(e.to_string()))?);
    }
    Ok(results)
}

pub fn list_all_by_project(
    conn: &Connection,
    project_id: &str,
) -> Result<Vec<Memory>, Trans4mersError> {
    let now_str = Utc::now().to_rfc3339();
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, conversation_id, agent_instance_id, scope, lifecycle,
                content, importance, confidence, provenance, depth_level, tier,
                retrieval_count, last_retrieved_at, expires_at, visibility_overrides,
                created_at, updated_at, valid_from, valid_until, embedding,
                valid_at, invalid_at, replaced_by, content_hash
         FROM project_memories
         WHERE project_id = ?1
           AND (valid_from IS NULL OR valid_from <= ?2)
           AND (invalid_at IS NULL OR invalid_at > ?2)
           AND (valid_until IS NULL OR valid_until > ?2)
           AND (expires_at IS NULL OR expires_at > datetime('now'))
         ORDER BY created_at DESC",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let rows = stmt
        .query_map(params![project_id, now_str], row_to_memory)
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut results = Vec::new();
    for r in rows {
        results.push(r.map_err(|e| Trans4mersError::Database(e.to_string()))?);
    }
    Ok(results)
}

pub fn invalidate_memory(
    conn: &Connection,
    id: &str,
    replaced_by: Option<&str>,
    invalid_at: Option<chrono::DateTime<Utc>>,
) -> Result<(), Trans4mersError> {
    let ts = invalid_at.unwrap_or_else(Utc::now).to_rfc3339();
    conn.execute(
        "UPDATE project_memories SET invalid_at = ?1, valid_until = ?1, replaced_by = ?2, updated_at = ?1 WHERE id = ?3",
        params![ts, replaced_by, id],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}

pub fn find_by_content_hash(
    conn: &Connection,
    project_id: &str,
    hash: &str,
) -> Result<Option<Memory>, Trans4mersError> {
    let query = "SELECT id, project_id, conversation_id, agent_instance_id, scope, lifecycle,
                        content, importance, confidence, provenance, depth_level, tier,
                        retrieval_count, last_retrieved_at, expires_at, visibility_overrides,
                        created_at, updated_at, valid_from, valid_until, embedding,
                        valid_at, invalid_at, replaced_by, content_hash
                 FROM project_memories
                 WHERE project_id = ?1 AND content_hash = ?2
                   AND (valid_from IS NULL OR valid_from <= datetime('now'))
                   AND (valid_until IS NULL OR valid_until > datetime('now'))
                   AND (invalid_at IS NULL OR invalid_at > datetime('now'))
                 ORDER BY created_at DESC LIMIT 1";
    let mut stmt = conn
        .prepare(query)
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    let mut rows = stmt
        .query(params![project_id, hash])
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    if let Some(row) = rows
        .next()
        .map_err(|e| Trans4mersError::Database(e.to_string()))?
    {
        Ok(Some(row_to_memory(row)?))
    } else {
        Ok(None)
    }
}

pub fn list_temporal_memories(
    conn: &Connection,
    project_id: &str,
) -> Result<Vec<Memory>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, conversation_id, agent_instance_id, scope, lifecycle,
                content, importance, confidence, provenance, depth_level, tier,
                retrieval_count, last_retrieved_at, expires_at, visibility_overrides,
                created_at, updated_at, valid_from, valid_until, embedding,
                valid_at, invalid_at, replaced_by, content_hash
         FROM project_memories
         WHERE project_id = ?1
         ORDER BY created_at DESC",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let rows = stmt
        .query_map(params![project_id], row_to_memory)
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut results = Vec::new();
    for r in rows {
        results.push(r.map_err(|e| Trans4mersError::Database(e.to_string()))?);
    }
    Ok(results)
}

/// Loads all project memory IDs and raw vector embeddings from SQLite relational table.
pub fn load_all_memory_embeddings(
    conn: &Connection,
    project_id: &str,
) -> Result<Vec<(String, Vec<f32>)>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, embedding
             FROM project_memories
             WHERE project_id = ?1 AND embedding IS NOT NULL",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let rows = stmt
        .query_map(params![project_id], |row| {
            let id: String = row.get(0)?;
            let blob: Vec<u8> = row.get(1)?;
            let emb = crate::vector_blob::blob_to_embedding(&blob);
            Ok((id, emb))
        })
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut results = Vec::new();
    for r in rows {
        results.push(r.map_err(|e| Trans4mersError::Database(e.to_string()))?);
    }
    Ok(results)
}

pub fn update_tier(
    conn: &Connection,
    id: &str,
    new_tier: MemoryTier,
) -> Result<(), Trans4mersError> {
    conn.execute(
        "UPDATE project_memories SET tier = ?1, updated_at = ?2 WHERE id = ?3",
        params![new_tier.to_string(), Utc::now().to_rfc3339(), id],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}

pub fn increment_retrieval_count(conn: &Connection, id: &str) -> Result<(), Trans4mersError> {
    conn.execute(
        "UPDATE project_memories
         SET retrieval_count = retrieval_count + 1, last_retrieved_at = ?1, updated_at = ?1
         WHERE id = ?2",
        params![Utc::now().to_rfc3339(), id],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}

pub fn reinforce_memory(
    conn: &Connection,
    id: &str,
    importance_bump: f32,
) -> Result<(), Trans4mersError> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE project_memories
         SET retrieval_count = retrieval_count + 1,
             importance = MIN(1.0, importance + ?1),
             last_retrieved_at = ?2,
             updated_at = ?2
         WHERE id = ?3",
        params![importance_bump, now, id],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}

pub fn delete_memory(conn: &Connection, id: &str) -> Result<(), Trans4mersError> {
    // 1. Clean up ghost vector from virtual table and mapping table
    if let Ok(rowid) = conn.query_row("SELECT rowid FROM vec_id_map WHERE uuid = ?1", [id], |r| {
        r.get::<_, i64>(0)
    }) {
        let _ = conn.execute("DELETE FROM vec_project_memories WHERE rowid = ?1", [rowid]);
        let _ = conn.execute("DELETE FROM vec_id_map WHERE rowid = ?1", [rowid]);
    }
    // 2. Delete main row
    conn.execute("DELETE FROM project_memories WHERE id = ?1", [id])
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}

pub fn count_by_tier(
    conn: &Connection,
    project_id: &str,
) -> Result<Vec<(String, usize, f32)>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT tier, COUNT(*), COALESCE(AVG(importance), 0.0)
         FROM project_memories
         WHERE project_id = ?1
         GROUP BY tier",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let rows = stmt
        .query_map([project_id], |row| {
            let tier: String = row.get(0)?;
            let count: usize = row.get(1)?;
            let avg_imp: f32 = row.get(2)?;
            Ok((tier, count, avg_imp))
        })
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut results = Vec::new();
    for r in rows {
        results.push(r.map_err(|e| Trans4mersError::Database(e.to_string()))?);
    }
    Ok(results)
}

fn row_to_memory(row: &rusqlite::Row) -> rusqlite::Result<Memory> {
    let id_str: String = row.get(0)?;
    let proj_str: String = row.get(1)?;
    let conv_str: Option<String> = row.get(2)?;
    let agent_str: Option<String> = row.get(3)?;
    let scope_str: String = row.get(4)?;
    let lifecycle_str: String = row.get(5)?;
    let content: String = row.get(6)?;
    let importance: f32 = row.get(7)?;
    let confidence: f32 = row.get(8)?;
    let provenance_str: String = row.get(9)?;
    let depth_level: u32 = row.get(10)?;
    let tier_str: String = row.get(11)?;
    let retrieval_count: u32 = row.get(12)?;
    let last_retrieved_str: Option<String> = row.get(13)?;
    let expires_str: Option<String> = row.get(14)?;
    let vis_str: Option<String> = row.get(15)?;
    let created_str: String = row.get(16)?;
    let updated_str: String = row.get(17)?;
    let valid_from_str: Option<String> = row.get(18)?;
    let valid_until_str: Option<String> = row.get(19)?;
    let embedding = row
        .get::<_, Option<Vec<u8>>>(20)
        .ok()
        .flatten()
        .map(|blob| crate::vector_blob::blob_to_embedding(&blob));
    let valid_at_str: Option<String> = row.get(21).ok().flatten();
    let invalid_at_str: Option<String> = row.get(22).ok().flatten();
    let replaced_by: Option<String> = row.get(23).ok().flatten();
    let content_hash: Option<String> = row.get(24).ok().flatten();

    Ok(Memory {
        id: MemoryId::from_str(&id_str).unwrap_or_default(),
        project_id: ProjectId::from_str(&proj_str).unwrap_or_default(),
        conversation_id: conv_str.and_then(|s| ConversationId::from_str(&s).ok()),
        agent_instance_id: agent_str.and_then(|s| AgentInstanceId::from_str(&s).ok()),
        scope: MemoryScope::from_str(&scope_str).unwrap_or(MemoryScope::Project),
        lifecycle: MemoryLifecycle::from_str(&lifecycle_str)
            .unwrap_or(MemoryLifecycle::Observation),
        content,
        embedding,
        importance,
        confidence,
        provenance: serde_json::from_str(&provenance_str).unwrap_or_else(|_| MemoryProvenance {
            source_event_id: None,
            source_message_id: None,
            source_agent_id: None,
            creation_reason: "Unknown".to_string(),
        }),
        depth_level,
        tier: MemoryTier::from_str(&tier_str).unwrap_or(MemoryTier::Working),
        retrieval_count,
        last_retrieved_at: crate::datetime_util::parse_db_datetime_opt(
            last_retrieved_str.as_deref(),
        ),
        expires_at: crate::datetime_util::parse_db_datetime_opt(expires_str.as_deref()),
        valid_from: crate::datetime_util::parse_db_datetime_opt(valid_from_str.as_deref()),
        valid_until: crate::datetime_util::parse_db_datetime_opt(valid_until_str.as_deref()),
        valid_at: crate::datetime_util::parse_db_datetime_opt(valid_at_str.as_deref())
            .or_else(|| crate::datetime_util::parse_db_datetime_opt(valid_from_str.as_deref())),
        invalid_at: crate::datetime_util::parse_db_datetime_opt(invalid_at_str.as_deref())
            .or_else(|| crate::datetime_util::parse_db_datetime_opt(valid_until_str.as_deref())),
        replaced_by,
        content_hash,
        visibility_overrides: vis_str.and_then(|s| serde_json::from_str(&s).ok()),
        created_at: crate::datetime_util::parse_db_datetime(&created_str),
        updated_at: crate::datetime_util::parse_db_datetime(&updated_str),
    })
}
