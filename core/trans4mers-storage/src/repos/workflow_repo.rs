use rusqlite::{Connection, OptionalExtension, params};
use serde_json;
use std::str::FromStr;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::ids::{
    ConversationId, ProjectId, WorkflowId, WorkflowNodeId, WorkflowRunId,
};
use trans4mers_domain::workflow::{Workflow, WorkflowRun, WorkflowRunStatus};

pub fn insert_workflow(conn: &Connection, wf: &Workflow) -> Result<(), Trans4mersError> {
    let nodes_json = serde_json::to_string(&wf.nodes)
        .map_err(|e| Trans4mersError::Database(format!("Failed to serialize workflow nodes: {}", e)))?;
    let edges_json = serde_json::to_string(&wf.edges)
        .map_err(|e| Trans4mersError::Database(format!("Failed to serialize workflow edges: {}", e)))?;

    conn.execute(
        "INSERT INTO workflows (
            id, conversation_id, name, description, nodes, edges, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            wf.id.as_str(),
            wf.conversation_id.as_str(),
            wf.name,
            wf.description,
            nodes_json,
            edges_json,
            wf.created_at.to_rfc3339(),
            wf.updated_at.to_rfc3339(),
        ],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}

pub fn insert_workflow_run(conn: &Connection, run: &WorkflowRun) -> Result<(), Trans4mersError> {
    conn.execute(
        "INSERT INTO workflow_runs (
            id, workflow_id, status, current_node_id, started_at, completed_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            run.id.as_str(),
            run.workflow_id.as_str(),
            run.status.to_string(),
            run.current_node_id.as_ref().map(|id| id.as_str()),
            run.started_at.to_rfc3339(),
            run.completed_at.map(|d| d.to_rfc3339()),
        ],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}

pub fn get_workflow(
    conn: &Connection,
    id: &WorkflowId,
) -> Result<Option<Workflow>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, conversation_id, name, description, nodes, edges, created_at, updated_at
         FROM workflows WHERE id = ?1",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let wf = stmt
        .query_row(params![id.as_str()], |row| {
            let nodes_str: String = row.get(4)?;
            let edges_str: String = row.get(5)?;

            let nodes = serde_json::from_str(&nodes_str).unwrap_or_default();
            let edges = serde_json::from_str(&edges_str).unwrap_or_default();

            Ok(Workflow {
                id: WorkflowId::from_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                conversation_id: ConversationId::from_str(&row.get::<_, String>(1)?)
                    .unwrap_or_default(),
                name: row.get(2)?,
                description: row.get(3)?,
                nodes,
                edges,
                created_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(6)?),
                updated_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(7)?),
            })
        })
        .optional()
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    Ok(wf)
}

pub fn list_workflows(
    conn: &Connection,
    project_id: &ProjectId,
) -> Result<Vec<Workflow>, Trans4mersError> {
    let mut stmt = conn.prepare(
        "SELECT w.id, w.conversation_id, w.name, w.description, w.nodes, w.edges, w.created_at, w.updated_at
         FROM workflows w
         JOIN conversations c ON w.conversation_id = c.id
         WHERE c.project_id = ?1"
    ).map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let rows = stmt
        .query_map(params![project_id.as_str()], |row| {
            let nodes_str: String = row.get(4)?;
            let edges_str: String = row.get(5)?;

            let nodes = serde_json::from_str(&nodes_str).unwrap_or_default();
            let edges = serde_json::from_str(&edges_str).unwrap_or_default();

            Ok(Workflow {
                id: WorkflowId::from_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                conversation_id: ConversationId::from_str(&row.get::<_, String>(1)?)
                    .unwrap_or_default(),
                name: row.get(2)?,
                description: row.get(3)?,
                nodes,
                edges,
                created_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(6)?),
                updated_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(7)?),
            })
        })
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut wfs = Vec::new();
    for row in rows {
        wfs.push(row.map_err(|e| Trans4mersError::Database(e.to_string()))?);
    }
    Ok(wfs)
}

pub fn get_workflow_run(
    conn: &Connection,
    id: &WorkflowRunId,
) -> Result<Option<WorkflowRun>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, workflow_id, status, current_node_id, started_at, completed_at
         FROM workflow_runs WHERE id = ?1",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let run = stmt
        .query_row(params![id.as_str()], |row| {
            let status_str: String = row.get(2)?;
            Ok(WorkflowRun {
                id: WorkflowRunId::from_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                workflow_id: WorkflowId::from_str(&row.get::<_, String>(1)?).unwrap_or_default(),
                status: WorkflowRunStatus::from_str(&status_str)
                    .unwrap_or(WorkflowRunStatus::Running),
                current_node_id: row
                    .get::<_, Option<String>>(3)?
                    .and_then(|s| WorkflowNodeId::from_str(&s).ok()),
                started_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(4)?),
                completed_at: crate::datetime_util::parse_db_datetime_opt(
                    row.get::<_, Option<String>>(5)?.as_deref(),
                ),
            })
        })
        .optional()
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    Ok(run)
}

pub fn update_workflow_run_status(
    conn: &Connection,
    id: &WorkflowRunId,
    status: WorkflowRunStatus,
) -> Result<(), Trans4mersError> {
    let now = chrono::Utc::now().to_rfc3339();
    let completed = match status {
        WorkflowRunStatus::Completed | WorkflowRunStatus::Failed | WorkflowRunStatus::Cancelled => {
            Some(now.clone())
        }
        _ => None,
    };

    conn.execute(
        "UPDATE workflow_runs SET status = ?1, completed_at = COALESCE(completed_at, ?2) WHERE id = ?3",
        params![
            status.to_string(),
            completed,
            id.as_str()
        ],
    ).map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}
