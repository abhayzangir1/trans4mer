use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use trans4mers_domain::error::Trans4mersError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTask {
    pub id: String,
    pub project_id: String,
    pub conversation_id: String,
    pub target_agent_id: String,
    pub cron_expression: String,
    pub human_readable: String,
    pub action_prompt: String,
    pub is_active: bool,
    pub last_run_at: Option<DateTime<Utc>>,
    pub next_run_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

pub fn create_task(conn: &Connection, task: &ScheduledTask) -> Result<(), Trans4mersError> {
    conn.execute(
        "INSERT INTO scheduled_tasks (
            id, project_id, conversation_id, target_agent_id, cron_expression,
            human_readable, action_prompt, is_active, last_run_at, next_run_at, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            task.id,
            task.project_id,
            task.conversation_id,
            task.target_agent_id,
            task.cron_expression,
            task.human_readable,
            task.action_prompt,
            if task.is_active { 1 } else { 0 },
            task.last_run_at.map(|d| d.to_rfc3339()),
            task.next_run_at.map(|d| d.to_rfc3339()),
            task.created_at.to_rfc3339(),
        ],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}

pub fn list_tasks(
    conn: &Connection,
    project_id: &str,
) -> Result<Vec<ScheduledTask>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, conversation_id, target_agent_id, cron_expression,
                human_readable, action_prompt, is_active, last_run_at, next_run_at, created_at
         FROM scheduled_tasks WHERE project_id = ?1 ORDER BY created_at DESC",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let rows = stmt
        .query_map(params![project_id], |row| {
            let is_active_int: i32 = row.get(7)?;
            let last_run_str: Option<String> = row.get(8)?;
            let next_run_str: Option<String> = row.get(9)?;
            let created_str: String = row.get(10)?;

            let last_run_at = crate::datetime_util::parse_db_datetime_opt(last_run_str.as_deref());
            let next_run_at = crate::datetime_util::parse_db_datetime_opt(next_run_str.as_deref());
            let created_at = crate::datetime_util::parse_db_datetime(&created_str);

            Ok(ScheduledTask {
                id: row.get(0)?,
                project_id: row.get(1)?,
                conversation_id: row.get(2)?,
                target_agent_id: row.get(3)?,
                cron_expression: row.get(4)?,
                human_readable: row.get(5)?,
                action_prompt: row.get(6)?,
                is_active: is_active_int == 1,
                last_run_at,
                next_run_at,
                created_at,
            })
        })
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut result = Vec::new();
    for r in rows {
        result.push(r.map_err(|e| Trans4mersError::Database(e.to_string()))?);
    }
    Ok(result)
}

pub fn list_due_tasks(
    conn: &Connection,
    now: DateTime<Utc>,
) -> Result<Vec<ScheduledTask>, Trans4mersError> {
    let now_str = now.to_rfc3339();
    let mut stmt = conn.prepare(
        "SELECT id, project_id, conversation_id, target_agent_id, cron_expression,
                human_readable, action_prompt, is_active, last_run_at, next_run_at, created_at
         FROM scheduled_tasks WHERE is_active = 1 AND next_run_at IS NOT NULL AND next_run_at <= ?1"
    ).map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let rows = stmt
        .query_map(params![now_str], |row| {
            let is_active_int: i32 = row.get(7)?;
            let last_run_str: Option<String> = row.get(8)?;
            let next_run_str: Option<String> = row.get(9)?;
            let created_str: String = row.get(10)?;

            let last_run_at = crate::datetime_util::parse_db_datetime_opt(last_run_str.as_deref());
            let next_run_at = crate::datetime_util::parse_db_datetime_opt(next_run_str.as_deref());
            let created_at = crate::datetime_util::parse_db_datetime(&created_str);

            Ok(ScheduledTask {
                id: row.get(0)?,
                project_id: row.get(1)?,
                conversation_id: row.get(2)?,
                target_agent_id: row.get(3)?,
                cron_expression: row.get(4)?,
                human_readable: row.get(5)?,
                action_prompt: row.get(6)?,
                is_active: is_active_int == 1,
                last_run_at,
                next_run_at,
                created_at,
            })
        })
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut result = Vec::new();
    for r in rows {
        result.push(r.map_err(|e| Trans4mersError::Database(e.to_string()))?);
    }
    Ok(result)
}

pub fn update_task_run(
    conn: &Connection,
    id: &str,
    last_run_at: DateTime<Utc>,
    next_run_at: Option<DateTime<Utc>>,
) -> Result<(), Trans4mersError> {
    conn.execute(
        "UPDATE scheduled_tasks SET last_run_at = ?1, next_run_at = ?2 WHERE id = ?3",
        params![
            last_run_at.to_rfc3339(),
            next_run_at.map(|d| d.to_rfc3339()),
            id,
        ],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}

pub fn toggle_task(conn: &Connection, id: &str, is_active: bool) -> Result<(), Trans4mersError> {
    conn.execute(
        "UPDATE scheduled_tasks SET is_active = ?1 WHERE id = ?2",
        params![if is_active { 1 } else { 0 }, id],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}

pub fn delete_task(conn: &Connection, id: &str) -> Result<(), Trans4mersError> {
    conn.execute("DELETE FROM scheduled_tasks WHERE id = ?1", params![id])
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}
