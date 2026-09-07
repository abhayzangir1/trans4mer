use rusqlite::OptionalExtension;
use tauri::State;
use trans4mers_domain::execution::AgentExecution;
use trans4mers_domain::ids::{AgentInstanceId, ConversationId, ExecutionId, ProjectId};
use trans4mers_domain::state::ExecutionStatus;
use trans4mers_engine::app_state::AppState;

#[tauri::command]
pub async fn list_active_executions(
    project_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<AgentExecution>, String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let db = state
        .get_project_db(&proj_id)
        .ok_or_else(|| "Project DB not found".to_string())?;

    let execs: Vec<AgentExecution> = db.with_read_conn(|conn| {
        let mut stmt = conn.prepare("SELECT id, agent_instance_id, conversation_id, status, generation, current_step, max_steps, started_at, updated_at FROM agent_executions WHERE status = 'Running' OR status = 'Queued' ORDER BY updated_at DESC")?;

        let iter = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let agent_instance_id: String = row.get(1)?;
            let conversation_id: String = row.get(2)?;
            let status: String = row.get(3)?;
            let generation: u64 = row.get(4)?;
            let current_step: u32 = row.get(5)?;
            let max_steps: u32 = row.get(6)?;
            let started_at: Option<String> = row.get(7)?;
            let updated_at: String = row.get(8)?;

            let exec_id = ExecutionId::from_str(&id)
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?;
            let ag_id = AgentInstanceId::from_str(&agent_instance_id)
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e)))?;
            let convo_id = ConversationId::from_str(&conversation_id)
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e)))?;
            let started = started_at.map(|s| trans4mers_storage::parse_db_datetime(&s));
            let updated = trans4mers_storage::parse_db_datetime(&updated_at);

            Ok(AgentExecution {
                id: exec_id,
                agent_instance_id: ag_id,
                task_id: None,
                conversation_id: convo_id,
                status: std::str::FromStr::from_str(&status).unwrap_or(ExecutionStatus::Queued),
                generation,
                current_step,
                max_steps,
                trigger_message_id: None,
                started_at: started,
                updated_at: updated,
                completed_at: None,
            })
        })?;

        let mut execs = Vec::new();
        for ex in iter.flatten() { execs.push(ex); }
        Ok(execs)
    })?;

    Ok(execs)
}

#[tauri::command]
pub async fn cancel_execution(
    execution_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let id = ExecutionId::from_str(&execution_id).map_err(|e| e.to_string())?;
    state.scheduler.cancel_execution(&id, state.inner());
    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct AgentExecutionDetails {
    pub execution: Option<AgentExecution>,
    pub steps: Vec<trans4mers_domain::execution::ReActStep>,
}

#[tauri::command]
pub async fn get_agent_execution_details(
    project_id: String,
    agent_id: String,
    state: State<'_, AppState>,
) -> Result<AgentExecutionDetails, String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let db = state
        .get_project_db(&proj_id)
        .ok_or_else(|| "Project DB not found".to_string())?;

    let details = db.with_read_conn(|conn| {
        // 1. Get latest execution for this agent
        let mut exec_stmt = conn.prepare(
            "SELECT id, agent_instance_id, conversation_id, status, generation, current_step, max_steps, started_at, updated_at, completed_at
             FROM agent_executions
             WHERE agent_instance_id = ?1
             ORDER BY updated_at DESC
             LIMIT 1"
        )?;

        let exec_opt = exec_stmt.query_row(rusqlite::params![&agent_id], |row| {
            let id: String = row.get(0)?;
            let ag_id: String = row.get(1)?;
            let convo_id: String = row.get(2)?;
            let status: String = row.get(3)?;
            let generation: u64 = row.get(4)?;
            let current_step: u32 = row.get(5)?;
            let max_steps: u32 = row.get(6)?;
            let started_at: Option<String> = row.get(7)?;
            let updated_at: String = row.get(8)?;
            let completed_at: Option<String> = row.get(9)?;

            let exec_id = ExecutionId::from_str(&id).map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?;
            let ag_instance_id = AgentInstanceId::from_str(&ag_id).map_err(|e| rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e)))?;
            let conversation_id = ConversationId::from_str(&convo_id).map_err(|e| rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e)))?;

            let started = started_at.map(|s| trans4mers_storage::parse_db_datetime(&s));
            let updated = trans4mers_storage::parse_db_datetime(&updated_at);
            let completed = completed_at.map(|s| trans4mers_storage::parse_db_datetime(&s));

            Ok(AgentExecution {
                id: exec_id,
                agent_instance_id: ag_instance_id,
                task_id: None,
                conversation_id,
                status: std::str::FromStr::from_str(&status).unwrap_or(ExecutionStatus::Queued),
                generation,
                current_step,
                max_steps,
                trigger_message_id: None,
                started_at: started,
                updated_at: updated,
                completed_at: completed,
            })
        }).optional()?;

        // 2. If execution exists, query latest checkpoint context_snapshot
        let mut steps = Vec::new();
        if let Some(ref exec) = exec_opt {
            let mut cp_stmt = conn.prepare(
                "SELECT context_snapshot FROM execution_checkpoints WHERE execution_id = ?1 ORDER BY step_number DESC LIMIT 1"
            )?;
            let snapshot_opt: Option<String> = cp_stmt.query_row(rusqlite::params![exec.id.as_str()], |row| row.get(0)).optional()?;
            if let Some(snapshot) = snapshot_opt
                && let Ok(st) = serde_json::from_str::<trans4mers_domain::execution::ExecutionState>(&snapshot) {
                    steps = st.steps;
                }
        }

        Ok(AgentExecutionDetails {
            execution: exec_opt,
            steps,
        })
    }).map_err(|e| e.to_string())?;

    Ok(details)
}
