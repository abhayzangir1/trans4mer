use rusqlite::{Connection, OptionalExtension, params};
use std::str::FromStr;
use trans4mers_domain::approval::{Approval, ApprovalStatus};
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::ids::{AgentInstanceId, ApprovalId, ConversationId, ExecutionId, ProjectId};

pub fn insert_approval(conn: &Connection, app: &Approval) -> Result<(), Trans4mersError> {
    conn.execute(
        "INSERT INTO approvals (
            id, execution_id, agent_instance_id, conversation_id,
            capability, tool_name, action_description, arguments_summary, arguments_hash,
            risk_level, status, human_feedback, requested_at, expires_at, resolved_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            app.id.as_str(),
            app.execution_id.as_str(),
            app.agent_instance_id.as_str(),
            app.conversation_id.as_str(),
            app.capability.to_string(),
            app.tool_name,
            app.action_description,
            app.arguments_summary,
            app.arguments_hash,
            app.risk_level.to_string(),
            app.status.to_string(),
            app.human_feedback,
            app.requested_at.to_rfc3339(),
            app.expires_at.to_rfc3339(),
            app.resolved_at.map(|d| d.to_rfc3339()),
        ],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}

pub fn resolve_approval(
    conn: &Connection,
    id: &ApprovalId,
    status: trans4mers_domain::approval::ApprovalStatus,
    feedback: Option<String>,
) -> Result<(), Trans4mersError> {
    conn.execute(
        "UPDATE approvals SET status = ?1, human_feedback = ?2, resolved_at = ?3 WHERE id = ?4",
        params![
            status.to_string(),
            feedback,
            chrono::Utc::now().to_rfc3339(),
            id.as_str()
        ],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}

pub fn has_approved(
    conn: &Connection,
    execution_id: &ExecutionId,
    tool_name: &str,
    arguments_hash: &str,
) -> Result<bool, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT status FROM approvals
             WHERE execution_id = ?1 AND tool_name = ?2
               AND arguments_hash = ?3",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    let rows = stmt
        .query_map(
            params![execution_id.as_str(), tool_name, arguments_hash],
            |r| r.get::<_, String>(0),
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    for r in rows {
        let status = r.map_err(|e| Trans4mersError::Database(e.to_string()))?;
        if status == "Approved" {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn was_rejected(
    conn: &Connection,
    execution_id: &ExecutionId,
    tool_name: &str,
    arguments_hash: &str,
) -> Result<bool, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT status FROM approvals
             WHERE execution_id = ?1 AND tool_name = ?2
               AND arguments_hash = ?3",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    let rows = stmt
        .query_map(
            params![execution_id.as_str(), tool_name, arguments_hash],
            |r| r.get::<_, String>(0),
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    for r in rows {
        let status = r.map_err(|e| Trans4mersError::Database(e.to_string()))?;
        if status == "Rejected" {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn expire_old_approvals(conn: &Connection) -> Result<u32, Trans4mersError> {
    let now = chrono::Utc::now().to_rfc3339();
    let n = conn
        .execute(
            "UPDATE approvals SET status = 'Expired', resolved_at = ?1
         WHERE status = 'Pending' AND expires_at < ?1",
            params![now],
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(n as u32)
}

pub fn get_approval(
    conn: &Connection,
    id: &ApprovalId,
) -> Result<Option<Approval>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, execution_id, agent_instance_id, conversation_id,
                capability, tool_name, action_description, arguments_summary, arguments_hash,
                risk_level, status, human_feedback, requested_at, expires_at, resolved_at
         FROM approvals WHERE id = ?1",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let app = stmt
        .query_row(params![id.as_str()], |row| {
            Ok(Approval {
                id: ApprovalId::from_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                execution_id: ExecutionId::from_str(&row.get::<_, String>(1)?).unwrap_or_default(),
                agent_instance_id: AgentInstanceId::from_str(&row.get::<_, String>(2)?)
                    .unwrap_or_default(),
                conversation_id: ConversationId::from_str(&row.get::<_, String>(3)?)
                    .unwrap_or_default(),
                capability: trans4mers_domain::tool::Capability::from_str(
                    &row.get::<_, String>(4)?,
                )
                .unwrap_or(trans4mers_domain::tool::Capability::Custom(
                    "Unknown".to_string(),
                )),
                tool_name: row.get(5)?,
                action_description: row.get(6)?,
                arguments_summary: row.get(7)?,
                arguments_hash: row.get(8)?,
                risk_level: trans4mers_domain::tool::RiskLevel::from_str(&row.get::<_, String>(9)?)
                    .unwrap_or(trans4mers_domain::tool::RiskLevel::Low),
                status: ApprovalStatus::from_str(&row.get::<_, String>(10)?)
                    .unwrap_or(ApprovalStatus::Pending),
                human_feedback: row.get(11)?,
                requested_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(12)?),
                expires_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(13)?),
                resolved_at: crate::datetime_util::parse_db_datetime_opt(
                    row.get::<_, Option<String>>(14)?.as_deref(),
                ),
            })
        })
        .optional()
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    Ok(app)
}

pub fn list_pending_approvals(
    conn: &Connection,
    project_id: &ProjectId,
) -> Result<Vec<Approval>, Trans4mersError> {
    let mut stmt = conn.prepare(
        "SELECT a.id, a.execution_id, a.agent_instance_id, a.conversation_id,
                a.capability, a.tool_name, a.action_description, a.arguments_summary, a.arguments_hash,
                a.risk_level, a.status, a.human_feedback, a.requested_at, a.expires_at, a.resolved_at
         FROM approvals a
         JOIN conversations c ON a.conversation_id = c.id
         WHERE c.project_id = ?1 AND a.status = 'Pending'"
    ).map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let rows = stmt
        .query_map(params![project_id.as_str()], |row| {
            Ok(Approval {
                id: ApprovalId::from_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                execution_id: ExecutionId::from_str(&row.get::<_, String>(1)?).unwrap_or_default(),
                agent_instance_id: AgentInstanceId::from_str(&row.get::<_, String>(2)?)
                    .unwrap_or_default(),
                conversation_id: ConversationId::from_str(&row.get::<_, String>(3)?)
                    .unwrap_or_default(),
                capability: trans4mers_domain::tool::Capability::from_str(
                    &row.get::<_, String>(4)?,
                )
                .unwrap_or(trans4mers_domain::tool::Capability::Custom(
                    "Unknown".to_string(),
                )),
                tool_name: row.get(5)?,
                action_description: row.get(6)?,
                arguments_summary: row.get(7)?,
                arguments_hash: row.get(8)?,
                risk_level: trans4mers_domain::tool::RiskLevel::from_str(&row.get::<_, String>(9)?)
                    .unwrap_or(trans4mers_domain::tool::RiskLevel::Low),
                status: ApprovalStatus::from_str(&row.get::<_, String>(10)?)
                    .unwrap_or(ApprovalStatus::Pending),
                human_feedback: row.get(11)?,
                requested_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(12)?),
                expires_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(13)?),
                resolved_at: crate::datetime_util::parse_db_datetime_opt(
                    row.get::<_, Option<String>>(14)?.as_deref(),
                ),
            })
        })
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut apps = Vec::new();
    for row in rows {
        apps.push(row.map_err(|e| Trans4mersError::Database(e.to_string()))?);
    }
    Ok(apps)
}
