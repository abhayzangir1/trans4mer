use rusqlite::{Connection, OptionalExtension, params};
use std::str::FromStr;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::ids::{LearnedRuleId, ProjectId};
use trans4mers_domain::learning::{Confidence, LearnedRule, RuleMetadata};

pub fn insert_rule(conn: &Connection, rule: &LearnedRule) -> Result<(), Trans4mersError> {
    let metadata_json = serde_json::to_string(&rule.metadata).unwrap_or_else(|_| "{}".to_string());
    let embedding_bytes = rule
        .embedding
        .as_ref()
        .map(|e| crate::vector_blob::embedding_to_blob(e));

    conn.execute(
        "INSERT INTO learned_rules (
            id, project_id, rule_text, confidence, metadata, created_at, embedding
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            rule.id.as_str(),
            rule.project_id.as_ref().map(|id| id.as_str()),
            rule.rule_text,
            rule.confidence.to_string(),
            metadata_json,
            rule.created_at.to_rfc3339(),
            embedding_bytes,
        ],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}

pub fn get_rule(
    conn: &Connection,
    id: &LearnedRuleId,
) -> Result<Option<LearnedRule>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, rule_text, confidence, metadata, created_at, embedding
         FROM learned_rules WHERE id = ?1",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let rule = stmt
        .query_row(params![id.as_str()], |row| {
            let confidence_str: String = row.get(3)?;
            let metadata_str: String = row.get(4)?;
            let embedding = row
                .get::<_, Option<Vec<u8>>>(6)
                .ok()
                .flatten()
                .map(|b| crate::vector_blob::blob_to_embedding(&b));

            Ok(LearnedRule {
                id: LearnedRuleId::from_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                project_id: row
                    .get::<_, Option<String>>(1)?
                    .and_then(|s| ProjectId::from_str(&s).ok()),
                rule_text: row.get(2)?,
                confidence: Confidence::from_str(&confidence_str).unwrap_or(Confidence::Medium),
                metadata: serde_json::from_str(&metadata_str).unwrap_or(RuleMetadata {
                    language: None,
                    framework: None,
                    file_pattern: None,
                    source_agent_id: None,
                    source_execution_id: None,
                }),
                created_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(5)?),
                embedding,
            })
        })
        .optional()
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    Ok(rule)
}

pub fn list_rules_by_project(
    conn: &Connection,
    project_id: &ProjectId,
) -> Result<Vec<LearnedRule>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, rule_text, confidence, metadata, created_at, embedding
         FROM learned_rules WHERE project_id = ?1 OR project_id IS NULL",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let rows = stmt
        .query_map(params![project_id.as_str()], |row| {
            let confidence_str: String = row.get(3)?;
            let metadata_str: String = row.get(4)?;
            let embedding = row
                .get::<_, Option<Vec<u8>>>(6)
                .ok()
                .flatten()
                .map(|b| crate::vector_blob::blob_to_embedding(&b));

            Ok(LearnedRule {
                id: LearnedRuleId::from_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                project_id: row
                    .get::<_, Option<String>>(1)?
                    .and_then(|s| ProjectId::from_str(&s).ok()),
                rule_text: row.get(2)?,
                confidence: Confidence::from_str(&confidence_str).unwrap_or(Confidence::Medium),
                metadata: serde_json::from_str(&metadata_str).unwrap_or(RuleMetadata {
                    language: None,
                    framework: None,
                    file_pattern: None,
                    source_agent_id: None,
                    source_execution_id: None,
                }),
                created_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(5)?),
                embedding,
            })
        })
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut rules = Vec::new();
    for row in rows {
        rules.push(row.map_err(|e| Trans4mersError::Database(e.to_string()))?);
    }
    Ok(rules)
}

/// Loads all learned rule IDs and raw vector embeddings from SQLite relational table.
pub fn load_all_rule_embeddings(
    conn: &Connection,
    project_id: Option<&ProjectId>,
) -> Result<Vec<(String, Vec<f32>)>, Trans4mersError> {
    let (sql, params_vec): (&str, Vec<rusqlite::types::Value>) = match project_id {
        Some(pid) => (
            "SELECT id, embedding FROM learned_rules WHERE (project_id = ?1 OR project_id IS NULL) AND embedding IS NOT NULL",
            vec![rusqlite::types::Value::Text(pid.to_string())],
        ),
        None => (
            "SELECT id, embedding FROM learned_rules WHERE embedding IS NOT NULL",
            vec![],
        ),
    };

    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let rows = stmt
        .query_map(rusqlite::params_from_iter(params_vec), |row| {
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

pub fn delete_rule(conn: &Connection, id: &LearnedRuleId) -> Result<(), Trans4mersError> {
    let vec_store = crate::vector_store::SqliteVecStore::new();
    let id_str = id.as_str();
    let _ = vec_store.delete_embedding_conn(conn, "vec_learned_rules", &id_str);

    conn.execute("DELETE FROM learned_rules WHERE id = ?1", params![id_str])
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}
