use chrono::Utc;
use rusqlite::{Connection, params};
use std::str::FromStr;
use trans4mers_domain::diff::{ActionDiff, DiffDecision, DiffHunk, DiffKind};
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::ids::{AgentInstanceId, ApprovalId, ExecutionId, ProjectId};

pub fn insert_diff(conn: &Connection, diff: &ActionDiff) -> Result<(), Trans4mersError> {
    let kind_json = serde_json::to_string(&diff.kind)
        .map_err(|e| Trans4mersError::Serialization(e.to_string()))?;
    let hunks_json = serde_json::to_string(&diff.hunks)
        .map_err(|e| Trans4mersError::Serialization(e.to_string()))?;
    let resolved_at_str = diff.resolved_at.map(|d| d.to_rfc3339());

    conn.execute(
        "INSERT INTO action_diffs (
            id, project_id, execution_id, agent_instance_id, capability, risk_level,
            kind, diff_payload, hunks, decision, force_review_reason, approval_id,
            created_at, resolved_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        params![
            diff.id,
            diff.project_id.as_str(),
            diff.execution_id.as_ref().map(|id| id.as_str()),
            diff.agent_instance_id.as_ref().map(|id| id.as_str()),
            diff.capability,
            diff.risk_level,
            kind_json,
            diff.diff_payload,
            hunks_json,
            diff.decision.to_string(),
            diff.force_review_reason,
            diff.approval_id.as_ref().map(|id| id.as_str()),
            diff.created_at.to_rfc3339(),
            resolved_at_str,
        ],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}

pub fn get_diff(conn: &Connection, id: &str) -> Result<Option<ActionDiff>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, execution_id, agent_instance_id, capability, risk_level,
                kind, diff_payload, hunks, decision, force_review_reason, approval_id,
                created_at, resolved_at
         FROM action_diffs WHERE id = ?1",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut rows = stmt
        .query([id])
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    if let Some(row) = rows
        .next()
        .map_err(|e| Trans4mersError::Database(e.to_string()))?
    {
        Ok(Some(row_to_diff(row)?))
    } else {
        Ok(None)
    }
}

pub fn list_pending(
    conn: &Connection,
    project_id: &str,
) -> Result<Vec<ActionDiff>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, execution_id, agent_instance_id, capability, risk_level,
                kind, diff_payload, hunks, decision, force_review_reason, approval_id,
                created_at, resolved_at
         FROM action_diffs
         WHERE project_id = ?1 AND decision = 'Pending'
         ORDER BY created_at ASC",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let rows = stmt
        .query_map([project_id], row_to_diff)
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut results = Vec::new();
    for r in rows {
        results.push(r.map_err(|e| Trans4mersError::Database(e.to_string()))?);
    }
    Ok(results)
}

pub fn resolve_diff(
    conn: &Connection,
    id: &str,
    decision: DiffDecision,
    hunks: Option<&[DiffHunk]>,
) -> Result<(), Trans4mersError> {
    let now = Utc::now().to_rfc3339();
    if let Some(h) = hunks {
        let hunks_json =
            serde_json::to_string(h).map_err(|e| Trans4mersError::Serialization(e.to_string()))?;
        conn.execute(
            "UPDATE action_diffs SET decision = ?1, hunks = ?2, resolved_at = ?3 WHERE id = ?4",
            params![decision.to_string(), hunks_json, now, id],
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    } else {
        conn.execute(
            "UPDATE action_diffs SET decision = ?1, resolved_at = ?2 WHERE id = ?3",
            params![decision.to_string(), now, id],
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    }
    Ok(())
}

pub fn get_by_approval_id(
    conn: &Connection,
    approval_id: &str,
) -> Result<Option<ActionDiff>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, execution_id, agent_instance_id, capability, risk_level,
                kind, diff_payload, hunks, decision, force_review_reason, approval_id,
                created_at, resolved_at
         FROM action_diffs WHERE approval_id = ?1",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut rows = stmt
        .query([approval_id])
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    if let Some(row) = rows
        .next()
        .map_err(|e| Trans4mersError::Database(e.to_string()))?
    {
        Ok(Some(row_to_diff(row)?))
    } else {
        Ok(None)
    }
}

fn row_to_diff(row: &rusqlite::Row) -> rusqlite::Result<ActionDiff> {
    let id: String = row.get(0)?;
    let proj_str: String = row.get(1)?;
    let exec_str: Option<String> = row.get(2)?;
    let agent_str: Option<String> = row.get(3)?;
    let capability: String = row.get(4)?;
    let risk_level: String = row.get(5)?;
    let kind_json: String = row.get(6)?;
    let diff_payload: String = row.get(7)?;
    let hunks_json: Option<String> = row.get(8)?;
    let decision_str: String = row.get(9)?;
    let force_review_reason: Option<String> = row.get(10)?;
    let approval_str: Option<String> = row.get(11)?;
    let created_str: String = row.get(12)?;
    let resolved_str: Option<String> = row.get(13)?;

    let kind: DiffKind = serde_json::from_str(&kind_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let hunks: Vec<DiffHunk> = hunks_json
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    Ok(ActionDiff {
        id,
        project_id: ProjectId::from_str(&proj_str).unwrap_or_default(),
        execution_id: exec_str.and_then(|s| ExecutionId::from_str(&s).ok()),
        agent_instance_id: agent_str.and_then(|s| AgentInstanceId::from_str(&s).ok()),
        capability,
        risk_level,
        kind,
        diff_payload,
        hunks,
        decision: DiffDecision::from_str(&decision_str).unwrap_or(DiffDecision::Pending),
        force_review_reason,
        approval_id: approval_str.and_then(|s| ApprovalId::from_str(&s).ok()),
        created_at: crate::datetime_util::parse_db_datetime(&created_str),
        resolved_at: crate::datetime_util::parse_db_datetime_opt(resolved_str.as_deref()),
    })
}
