use rusqlite::{Connection, OptionalExtension, params};
use std::str::FromStr;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::ids::{AgentInstanceId, ConversationId, ProjectId, TaskId};
use trans4mers_domain::task::{Task, TaskStatus};

pub fn insert_task(conn: &Connection, task: &Task) -> Result<(), Trans4mersError> {
    conn.execute(
        "INSERT INTO tasks (
            id, conversation_id, assigned_agent_id, parent_task_id,
            title, description, status, priority, created_at, updated_at, completed_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            task.id.as_str(),
            task.conversation_id.as_str(),
            task.assigned_agent_id.as_ref().map(|id| id.as_str()),
            task.parent_task_id.as_ref().map(|id| id.as_str()),
            task.title,
            task.description,
            task.status.to_string(),
            task.priority,
            task.created_at.to_rfc3339(),
            task.updated_at.to_rfc3339(),
            task.completed_at.map(|d| d.to_rfc3339()),
        ],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}

pub fn update_task_status(
    conn: &Connection,
    id: &TaskId,
    status: TaskStatus,
) -> Result<(), Trans4mersError> {
    let now = chrono::Utc::now().to_rfc3339();
    let completed = if status == TaskStatus::Completed {
        Some(now.clone())
    } else {
        None
    };

    conn.execute(
        "UPDATE tasks SET status = ?1, updated_at = ?2, completed_at = ?3 WHERE id = ?4",
        params![status.to_string(), now, completed, id.as_str()],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}

pub fn get_task(conn: &Connection, id: &TaskId) -> Result<Option<Task>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, conversation_id, assigned_agent_id, parent_task_id,
                title, description, status, priority, created_at, updated_at, completed_at
         FROM tasks WHERE id = ?1",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let task = stmt
        .query_row(params![id.as_str()], |row| {
            let status_str: String = row.get(6)?;
            Ok(Task {
                id: TaskId::from_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                conversation_id: ConversationId::from_str(&row.get::<_, String>(1)?)
                    .unwrap_or_default(),
                assigned_agent_id: row
                    .get::<_, Option<String>>(2)?
                    .and_then(|s| AgentInstanceId::from_str(&s).ok()),
                parent_task_id: row
                    .get::<_, Option<String>>(3)?
                    .and_then(|s| TaskId::from_str(&s).ok()),
                title: row.get(4)?,
                description: row.get(5)?,
                status: TaskStatus::from_str(&status_str).unwrap_or(TaskStatus::Pending),
                priority: row.get(7)?,
                created_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(8)?),
                updated_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(9)?),
                completed_at: crate::datetime_util::parse_db_datetime_opt(
                    row.get::<_, Option<String>>(10)?.as_deref(),
                ),
            })
        })
        .optional()
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    Ok(task)
}

pub fn list_tasks(conn: &Connection, project_id: &ProjectId) -> Result<Vec<Task>, Trans4mersError> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.conversation_id, t.assigned_agent_id, t.parent_task_id,
                t.title, t.description, t.status, t.priority, t.created_at, t.updated_at, t.completed_at
         FROM tasks t
         JOIN conversations c ON t.conversation_id = c.id
         WHERE c.project_id = ?1"
    ).map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let rows = stmt
        .query_map(params![project_id.as_str()], |row| {
            let status_str: String = row.get(6)?;
            Ok(Task {
                id: TaskId::from_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                conversation_id: ConversationId::from_str(&row.get::<_, String>(1)?)
                    .unwrap_or_default(),
                assigned_agent_id: row
                    .get::<_, Option<String>>(2)?
                    .and_then(|s| AgentInstanceId::from_str(&s).ok()),
                parent_task_id: row
                    .get::<_, Option<String>>(3)?
                    .and_then(|s| TaskId::from_str(&s).ok()),
                title: row.get(4)?,
                description: row.get(5)?,
                status: TaskStatus::from_str(&status_str).unwrap_or(TaskStatus::Pending),
                priority: row.get(7)?,
                created_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(8)?),
                updated_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(9)?),
                completed_at: crate::datetime_util::parse_db_datetime_opt(
                    row.get::<_, Option<String>>(10)?.as_deref(),
                ),
            })
        })
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut tasks = Vec::new();
    for row in rows {
        tasks.push(row.map_err(|e| Trans4mersError::Database(e.to_string()))?);
    }
    Ok(tasks)
}
