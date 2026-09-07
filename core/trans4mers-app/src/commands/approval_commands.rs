use chrono::Utc;
use std::str::FromStr;
use tauri::State;
use trans4mers_domain::approval::Approval;
use trans4mers_domain::diff::{ActionDiff, DiffDecision, DiffHunk};
use trans4mers_domain::ids::{ProjectId, WorkflowId, WorkflowRunId};
use trans4mers_engine::app_state::AppState;
use trans4mers_storage::repos::diff_repo;

#[tauri::command]
pub async fn get_pending_approvals(
    project_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<Approval>, String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let db = state
        .get_project_db(&proj_id)
        .ok_or_else(|| "Project DB not found".to_string())?;
    let apps: Vec<Approval> = db.with_read_conn(|conn| {
        let mut stmt = conn.prepare("SELECT id, execution_id, agent_instance_id, conversation_id, capability, tool_name, action_description, arguments_summary, arguments_hash, risk_level, status, human_feedback, requested_at, expires_at, resolved_at FROM approvals WHERE status = 'Pending'")?;

        let iter = stmt.query_map([], |row| {
            let status_str: String = row.get(10)?;
            let status = std::str::FromStr::from_str(&status_str).unwrap_or(trans4mers_domain::approval::ApprovalStatus::Pending);

            let cap_str: String = row.get(4)?;
            let capability = std::str::FromStr::from_str(&cap_str).unwrap_or(trans4mers_domain::tool::Capability::ShellExecute);

            let risk_str: String = row.get(9)?;
            let risk_level = std::str::FromStr::from_str(&risk_str).unwrap_or(trans4mers_domain::tool::RiskLevel::Medium);

            let id_str: String = row.get(0)?;
            let exec_str: String = row.get(1)?;
            let ag_str: String = row.get(2)?;
            let convo_str: String = row.get(3)?;
            let args_hash: Option<String> = row.get(8)?;
            let req_str: String = row.get(12)?;
            let exp_str: String = row.get(13)?;
            let res_opt: Option<String> = row.get(14)?;

            let ap_id = std::str::FromStr::from_str(&id_str)
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?;
            let ex_id = std::str::FromStr::from_str(&exec_str)
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e)))?;
            let ag_id = std::str::FromStr::from_str(&ag_str)
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e)))?;
            let cv_id = std::str::FromStr::from_str(&convo_str)
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e)))?;
            let req_at = chrono::DateTime::parse_from_rfc3339(&req_str)
                .map(|d| d.into())
                .unwrap_or_else(|_| chrono::Utc::now());
            let exp_at = chrono::DateTime::parse_from_rfc3339(&exp_str)
                .map(|d| d.into())
                .unwrap_or_else(|_| chrono::Utc::now());
            let res_at = res_opt.and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok().map(|d| d.into()));

            Ok(Approval {
                id: ap_id,
                execution_id: ex_id,
                agent_instance_id: ag_id,
                conversation_id: cv_id,
                capability,
                tool_name: row.get(5)?,
                action_description: row.get(6)?,
                arguments_summary: row.get(7)?,
                arguments_hash: args_hash,
                risk_level,
                status,
                human_feedback: row.get(11)?,
                requested_at: req_at,
                expires_at: exp_at,
                resolved_at: res_at,
            })
        })?;

        let mut apps = Vec::new();
        for a in iter {
            apps.push(a?);
        }
        Ok(apps)
    }).map_err(|e| e.to_string())?;

    Ok(apps)
}

#[tauri::command]
pub async fn resolve_approval(
    project_id: String,
    approval_id: String,
    approved: bool,
    feedback: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    use trans4mers_domain::event::DomainEvent;

    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let db = state
        .get_project_db(&proj_id)
        .ok_or_else(|| "Project DB not found".to_string())?;

    let status_str = if approved { "Approved" } else { "Rejected" };
    let _ = db.with_write_tx(|conn| {
        conn.execute(
            "UPDATE approvals SET status = ?1, human_feedback = ?2, resolved_at = CURRENT_TIMESTAMP WHERE id = ?3",
            rusqlite::params![status_str, feedback.as_deref(), &approval_id],
        )?;
        Ok(())
    });

    let mut workflow_to_resume: Option<(String, String)> = None;
    let _ = db.with_read_conn(|conn| {
        if let Ok(execution_id) = conn.query_row(
            "SELECT execution_id FROM approvals WHERE id = ?1",
            rusqlite::params![&approval_id],
            |row| row.get::<_, String>(0),
        ) {
            // Check if it's a workflow
            if let Ok(wf_id) = conn.query_row(
                "SELECT workflow_id FROM workflow_runs WHERE id = ?1 AND status = 'Paused'",
                rusqlite::params![&execution_id],
                |row| row.get::<_, String>(0),
            ) {
                workflow_to_resume = Some((wf_id, execution_id));
            }
        }
        Ok(())
    });

    let event = DomainEvent::ApprovalResolved {
        approval_id: approval_id.clone(),
        approved,
        feedback: feedback.clone(),
    };

    let envelope = db
        .with_write_tx(|conn| {
            trans4mers_engine::cqrs::commit_event(conn, event, state.human_actor_id)
        })
        .map_err(|e| e.to_string())?;

    let env_arc = std::sync::Arc::new(envelope);
    let bus = state.get_event_bus(&proj_id);
    let _ = bus.publish(env_arc.clone());
    let _ = state.global_event_bus.publish(env_arc);

    if let Some((wf_id, run_id)) = workflow_to_resume {
        if approved {
            if let (Ok(w), Ok(r)) = (
                WorkflowId::from_str(&wf_id),
                WorkflowRunId::from_str(&run_id),
            ) {
                let engine = trans4mers_engine::workflow_engine::WorkflowEngine::new(
                    std::sync::Arc::new(state.inner().clone()),
                );
                engine.execute_workflow_run(proj_id, w, r).await;
            }
        } else {
            let _ = db.with_write_tx(|conn| {
                conn.execute(
                    "UPDATE workflow_runs SET status = 'Failed' WHERE id = ?1",
                    rusqlite::params![&run_id],
                )?;
                Ok(())
            });
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn get_action_diff_by_approval(
    project_id: String,
    approval_id: String,
    state: State<'_, AppState>,
) -> Result<Option<ActionDiff>, String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let db = state
        .get_project_db(&proj_id)
        .ok_or_else(|| "Project DB not found".to_string())?;

    db.with_read_conn(|conn| diff_repo::get_by_approval_id(conn, &approval_id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_pending_action_diffs(
    project_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<ActionDiff>, String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let db = state
        .get_project_db(&proj_id)
        .ok_or_else(|| "Project DB not found".to_string())?;

    db.with_read_conn(|conn| diff_repo::list_pending(conn, &project_id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn resolve_action_diff(
    project_id: String,
    diff_id: String,
    decision: String,
    hunks: Option<Vec<DiffHunk>>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let db = state
        .get_project_db(&proj_id)
        .ok_or_else(|| "Project DB not found".to_string())?;
    let dec = DiffDecision::from_str(&decision).map_err(|e| e.to_string())?;

    db.with_write_tx(|conn| diff_repo::resolve_diff(conn, &diff_id, dec, hunks.as_deref()))
        .map_err(|e| e.to_string())?;

    // CQRS event
    let event = trans4mers_domain::event::DomainEvent::DiffReviewResolved {
        project_id: proj_id,
        diff_id,
        decision,
    };
    let envelope = trans4mers_domain::event::EventEnvelope {
        sequence_id: 0,
        event_id: trans4mers_domain::ids::EventId::new(),
        event,
        actor_id: state.human_actor_id,
        signature: None,
        created_at: Utc::now(),
    };
    let env_arc = std::sync::Arc::new(envelope);
    let _ = state.get_event_bus(&proj_id).publish(env_arc.clone());
    let _ = state.global_event_bus.publish(env_arc);

    Ok(())
}
