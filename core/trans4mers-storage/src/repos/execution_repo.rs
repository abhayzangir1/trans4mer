use rusqlite::{Connection, params};
use serde_json;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::execution::{AgentExecution, Checkpoint, ExecutionStep};
use trans4mers_domain::ids::{AgentInstanceId, ConversationId, ExecutionId, MessageId, TaskId};

pub fn get_execution(
    conn: &Connection,
    id: &ExecutionId,
) -> Result<Option<AgentExecution>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, agent_instance_id, task_id, conversation_id, status,
                generation, current_step, max_steps, trigger_message_id,
                started_at, updated_at, completed_at
         FROM agent_executions WHERE id = ?1",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut rows = stmt
        .query_map(params![id.as_str()], |row| {
            let id_str: String = row.get(0)?;
            let agent_id_str: String = row.get(1)?;
            let task_id_str: Option<String> = row.get(2)?;
            let convo_id_str: String = row.get(3)?;
            let status_str: String = row.get(4)?;
            let generation: u64 = row.get(5)?;
            let current_step: u32 = row.get(6)?;
            let max_steps: u32 = row.get(7)?;
            let trigger_msg_str: Option<String> = row.get(8)?;
            let started_at_str: Option<String> = row.get(9)?;
            let updated_at_str: String = row.get(10)?;
            let completed_at_str: Option<String> = row.get(11)?;

            let exec_id = ExecutionId::from_str(&id_str).unwrap_or_default();
            let agent_instance_id = AgentInstanceId::from_str(&agent_id_str).unwrap_or_default();
            let task_id = task_id_str.and_then(|s| TaskId::from_str(&s).ok());
            let conversation_id = ConversationId::from_str(&convo_id_str).unwrap_or_default();
            let status = status_str
                .parse()
                .unwrap_or(trans4mers_domain::state::ExecutionStatus::Queued);
            let trigger_message_id = trigger_msg_str.and_then(|s| MessageId::from_str(&s).ok());
            let started_at = crate::datetime_util::parse_db_datetime_opt(started_at_str.as_deref());
            let updated_at = crate::datetime_util::parse_db_datetime(&updated_at_str);
            let completed_at =
                crate::datetime_util::parse_db_datetime_opt(completed_at_str.as_deref());

            Ok(AgentExecution {
                id: exec_id,
                agent_instance_id,
                task_id,
                conversation_id,
                status,
                generation,
                current_step,
                max_steps,
                trigger_message_id,
                started_at,
                updated_at,
                completed_at,
            })
        })
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    if let Some(r) = rows.next() {
        r.map(Some)
            .map_err(|e| Trans4mersError::Database(e.to_string()))
    } else {
        Ok(None)
    }
}

pub fn insert_execution(conn: &Connection, exec: &AgentExecution) -> Result<(), Trans4mersError> {
    conn.execute(
        "INSERT INTO agent_executions (
            id, agent_instance_id, task_id, conversation_id, status,
            generation, current_step, max_steps, trigger_message_id,
            started_at, updated_at, completed_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
        ON CONFLICT(id) DO UPDATE SET
            status = excluded.status,
            generation = excluded.generation,
            current_step = excluded.current_step,
            started_at = COALESCE(agent_executions.started_at, excluded.started_at),
            updated_at = excluded.updated_at",
        params![
            exec.id.as_str(),
            exec.agent_instance_id.as_str(),
            exec.task_id.as_ref().map(|id| id.as_str()),
            exec.conversation_id.as_str(),
            exec.status.to_string(),
            exec.generation,
            exec.current_step,
            exec.max_steps,
            exec.trigger_message_id.as_ref().map(|id| id.as_str()),
            exec.started_at.map(|d| d.to_rfc3339()),
            exec.updated_at.to_rfc3339(),
            exec.completed_at.map(|d| d.to_rfc3339()),
        ],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    Ok(())
}

pub fn update_execution_status(
    conn: &Connection,
    id: &ExecutionId,
    status: trans4mers_domain::state::ExecutionStatus,
    generation: u64,
) -> Result<(), Trans4mersError> {
    conn.execute(
        "UPDATE agent_executions SET status = ?1, generation = ?2, updated_at = ?3 WHERE id = ?4",
        params![
            status.to_string(),
            generation,
            chrono::Utc::now().to_rfc3339(),
            id.as_str()
        ],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}

pub fn insert_step(conn: &Connection, step: &ExecutionStep) -> Result<(), Trans4mersError> {
    let tool_call_json = step
        .tool_call_request
        .as_ref()
        .map(|r| serde_json::to_string(r).unwrap());
    let token_usage_json = step
        .token_usage
        .as_ref()
        .map(|t| serde_json::to_string(t).unwrap());

    conn.execute(
        "INSERT INTO execution_steps (
            id, execution_id, step_number, step_type, action_intent,
            tool_call_request, tool_result_summary, decision_rationale,
            token_usage, duration_ms, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            step.id.to_string(),
            step.execution_id.as_str(),
            step.step_number,
            step.step_type.to_string(),
            step.action_intent,
            tool_call_json,
            step.tool_result_summary,
            step.decision_rationale,
            token_usage_json,
            step.duration_ms,
            step.created_at.to_rfc3339(),
        ],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    Ok(())
}

pub fn get_step(
    conn: &Connection,
    id: uuid::Uuid,
) -> Result<Option<ExecutionStep>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, execution_id, step_number, step_type, action_intent,
                tool_call_request, tool_result_summary, decision_rationale,
                token_usage, duration_ms, created_at
         FROM execution_steps WHERE id = ?1",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut rows = stmt
        .query_map(params![id.to_string()], |row| {
            let id_str: String = row.get(0)?;
            let exec_id_str: String = row.get(1)?;
            let step_type_str: String = row.get(3)?;
            let tool_req_str: Option<String> = row.get(5)?;
            let token_usage_str: Option<String> = row.get(8)?;
            let created_at_str: String = row.get(10)?;

            let step_id = uuid::Uuid::parse_str(&id_str).unwrap_or_default();
            let execution_id = ExecutionId::from_str(&exec_id_str).unwrap_or_default();
            let step_type = std::str::FromStr::from_str(&step_type_str)
                .unwrap_or(trans4mers_domain::execution::StepType::Reasoning);
            let tool_call_request = tool_req_str.and_then(|s| serde_json::from_str(&s).ok());
            let token_usage = token_usage_str.and_then(|s| serde_json::from_str(&s).ok());
            let created_at = crate::datetime_util::parse_db_datetime(&created_at_str);

            Ok(ExecutionStep {
                id: step_id,
                execution_id,
                step_number: row.get(2)?,
                step_type,
                action_intent: row.get(4)?,
                tool_call_request,
                tool_result_summary: row.get(6)?,
                decision_rationale: row.get(7)?,
                token_usage,
                duration_ms: row.get(9)?,
                created_at,
            })
        })
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    if let Some(r) = rows.next() {
        r.map(Some)
            .map_err(|e| Trans4mersError::Database(e.to_string()))
    } else {
        Ok(None)
    }
}

pub fn list_steps(
    conn: &Connection,
    execution_id: &ExecutionId,
) -> Result<Vec<ExecutionStep>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, execution_id, step_number, step_type, action_intent,
                tool_call_request, tool_result_summary, decision_rationale,
                token_usage, duration_ms, created_at
         FROM execution_steps WHERE execution_id = ?1 ORDER BY step_number ASC",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let rows = stmt
        .query_map(params![execution_id.as_str()], |row| {
            let id_str: String = row.get(0)?;
            let exec_id_str: String = row.get(1)?;
            let step_type_str: String = row.get(3)?;
            let tool_req_str: Option<String> = row.get(5)?;
            let token_usage_str: Option<String> = row.get(8)?;
            let created_at_str: String = row.get(10)?;

            let step_id = uuid::Uuid::parse_str(&id_str).unwrap_or_default();
            let execution_id_val = ExecutionId::from_str(&exec_id_str).unwrap_or_default();
            let step_type = std::str::FromStr::from_str(&step_type_str)
                .unwrap_or(trans4mers_domain::execution::StepType::Reasoning);
            let tool_call_request = tool_req_str.and_then(|s| serde_json::from_str(&s).ok());
            let token_usage = token_usage_str.and_then(|s| serde_json::from_str(&s).ok());
            let created_at = crate::datetime_util::parse_db_datetime(&created_at_str);

            Ok(ExecutionStep {
                id: step_id,
                execution_id: execution_id_val,
                step_number: row.get(2)?,
                step_type,
                action_intent: row.get(4)?,
                tool_call_request,
                tool_result_summary: row.get(6)?,
                decision_rationale: row.get(7)?,
                token_usage,
                duration_ms: row.get(9)?,
                created_at,
            })
        })
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut steps = Vec::new();
    for row in rows {
        steps.push(row.map_err(|e| Trans4mersError::Database(e.to_string()))?);
    }
    Ok(steps)
}

pub fn upsert_checkpoint(conn: &Connection, cp: &Checkpoint) -> Result<(), Trans4mersError> {
    let pending_state_json = cp
        .pending_tool_state
        .as_ref()
        .map(|s| serde_json::to_string(s).unwrap());
    let context_snap_json = cp
        .context_snapshot
        .as_ref()
        .map(|s| serde_json::to_string(s).unwrap());

    conn.execute(
        "INSERT INTO execution_checkpoints (
            id, execution_id, generation, step_number, execution_status,
            execution_phase, inbox_cursor, pending_tool_state,
            last_event_sequence, context_snapshot, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
        ON CONFLICT(execution_id) DO UPDATE SET
            generation = excluded.generation,
            step_number = excluded.step_number,
            execution_status = excluded.execution_status,
            execution_phase = excluded.execution_phase,
            inbox_cursor = excluded.inbox_cursor,
            pending_tool_state = excluded.pending_tool_state,
            last_event_sequence = excluded.last_event_sequence,
            context_snapshot = excluded.context_snapshot,
            created_at = excluded.created_at
        ",
        params![
            cp.id.as_str(),
            cp.execution_id.as_str(),
            cp.generation,
            cp.step_number,
            cp.execution_status.to_string(),
            cp.execution_phase.to_string(),
            cp.inbox_cursor,
            pending_state_json,
            cp.last_event_sequence,
            context_snap_json,
            cp.created_at.to_rfc3339(),
        ],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    Ok(())
}

pub fn get_checkpoint(
    conn: &Connection,
    execution_id: &ExecutionId,
) -> Result<Option<Checkpoint>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, execution_id, generation, step_number, execution_status,
                execution_phase, inbox_cursor, pending_tool_state,
                last_event_sequence, context_snapshot, created_at
         FROM execution_checkpoints
         WHERE execution_id = ?1",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut rows = stmt
        .query(params![execution_id.as_str()])
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    if let Some(row) = rows
        .next()
        .map_err(|e| Trans4mersError::Database(e.to_string()))?
    {
        let id_str: String = row.get(0)?;
        let exec_str: String = row.get(1)?;
        let generation: i64 = row.get(2)?;
        let step_number: u32 = row.get(3)?;
        let status_str: String = row.get(4)?;
        let phase_str: String = row.get(5)?;
        let inbox_cursor: Option<String> = row.get(6)?;
        let pending_json: Option<String> = row.get(7)?;
        let last_seq: i64 = row.get(8)?;
        let context_json: Option<String> = row.get(9)?;
        let created_str: String = row.get(10)?;

        let id = trans4mers_domain::ids::CheckpointId::from_str(&id_str)
            .map_err(|e| Trans4mersError::Database(e.to_string()))?;
        let execution_id = ExecutionId::from_str(&exec_str)
            .map_err(|e| Trans4mersError::Database(e.to_string()))?;
        let execution_status = status_str.parse().map_err(|_| {
            Trans4mersError::Database(format!("Invalid execution status: {}", status_str))
        })?;
        let execution_phase = phase_str.parse().map_err(|_| {
            Trans4mersError::Database(format!("Invalid execution phase: {}", phase_str))
        })?;
        let pending_tool_state = pending_json.and_then(|s| serde_json::from_str(&s).ok());
        let context_snapshot = context_json.and_then(|s| serde_json::from_str(&s).ok());
        let created_at = crate::datetime_util::parse_db_datetime(&created_str);

        Ok(Some(Checkpoint {
            id,
            execution_id,
            generation: generation as u64,
            step_number,
            execution_status,
            execution_phase,
            inbox_cursor,
            pending_tool_state,
            last_event_sequence: last_seq,
            context_snapshot,
            created_at,
        }))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_execution_upsert_on_conflict() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "
            CREATE TABLE agent_executions (
                id TEXT PRIMARY KEY,
                agent_instance_id TEXT NOT NULL,
                task_id TEXT,
                conversation_id TEXT NOT NULL,
                status TEXT NOT NULL,
                generation INTEGER NOT NULL DEFAULT 0,
                current_step INTEGER NOT NULL DEFAULT 0,
                max_steps INTEGER NOT NULL DEFAULT 50,
                trigger_message_id TEXT,
                started_at DATETIME,
                updated_at DATETIME NOT NULL,
                completed_at DATETIME
            );
        ",
        )
        .unwrap();

        let id = ExecutionId::new();
        let agent_id = AgentInstanceId::new();
        let convo_id = ConversationId::new();

        // 1. Initial insert (as happens in message_commands.rs)
        conn.execute(
            "INSERT INTO agent_executions (id, agent_instance_id, conversation_id, status, generation, current_step, max_steps, updated_at) VALUES (?1, ?2, ?3, 'Pending', 0, 0, 100, ?4)",
            rusqlite::params![id.as_str(), agent_id.as_str(), convo_id.as_str(), chrono::Utc::now().to_rfc3339()]
        ).unwrap();

        // 2. Projector receives ExecutionStarted and calls insert_execution with the same ID
        let exec = AgentExecution {
            id: id.clone(),
            agent_instance_id: agent_id.clone(),
            task_id: None,
            conversation_id: convo_id.clone(),
            status: trans4mers_domain::state::ExecutionStatus::Running,
            generation: 1,
            current_step: 0,
            max_steps: 50,
            trigger_message_id: None,
            started_at: Some(chrono::Utc::now()),
            updated_at: chrono::Utc::now(),
            completed_at: None,
        };

        // Must succeed with ON CONFLICT DO UPDATE SET instead of UNIQUE constraint error
        let res = insert_execution(&conn, &exec);
        assert!(res.is_ok());

        // Verify status updated to Running
        let loaded = get_execution(&conn, &id).unwrap().unwrap();
        assert_eq!(
            loaded.status,
            trans4mers_domain::state::ExecutionStatus::Running
        );
        assert_eq!(loaded.generation, 1);
        assert!(loaded.started_at.is_some());
    }
}
