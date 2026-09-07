use chrono::Utc;
use tauri::State;
use trans4mers_domain::ids::ProjectId;
use trans4mers_engine::app_state::AppState;
use trans4mers_storage::repos::scheduled_task_repo::{self, ScheduledTask};
use uuid::Uuid;

#[tauri::command]
pub async fn list_scheduled_tasks(
    project_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<ScheduledTask>, String> {
    let pid = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let db = state
        .get_project_db(&pid)
        .ok_or_else(|| "Project DB not found".to_string())?;

    db.with_read_conn(|conn| scheduled_task_repo::list_tasks(conn, &project_id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_scheduled_task(
    project_id: String,
    conversation_id: Option<String>,
    target_agent_id: Option<String>,
    cron_expression: String,
    human_readable: String,
    action_prompt: String,
    state: State<'_, AppState>,
) -> Result<ScheduledTask, String> {
    let pid = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let db = state
        .get_project_db(&pid)
        .ok_or_else(|| "Project DB not found".to_string())?;

    let task_id = Uuid::new_v4().to_string();
    let now = Utc::now();
    let next_run_at =
        trans4mers_engine::scheduled_automation::ScheduledAutomationManager::compute_next_run(
            &cron_expression,
            now,
        )
        .or_else(|| Some(now + chrono::Duration::seconds(60)));

    let task = ScheduledTask {
        id: task_id,
        project_id: project_id.clone(),
        conversation_id: conversation_id
            .unwrap_or_else(|| trans4mers_domain::constants::DEFAULT_AUTOMATION_ROLE.to_string()),
        target_agent_id: target_agent_id
            .unwrap_or_else(|| trans4mers_domain::constants::DEFAULT_ORCHESTRATOR_ROLE.to_string()),
        cron_expression,
        human_readable,
        action_prompt,
        is_active: true,
        last_run_at: None,
        next_run_at,
        created_at: now,
    };

    db.with_write_tx(|conn| scheduled_task_repo::create_task(conn, &task))
        .map_err(|e| e.to_string())?;

    Ok(task)
}

#[tauri::command]
pub async fn toggle_scheduled_task(
    project_id: String,
    task_id: String,
    is_active: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let pid = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let db = state
        .get_project_db(&pid)
        .ok_or_else(|| "Project DB not found".to_string())?;

    db.with_write_tx(|conn| scheduled_task_repo::toggle_task(conn, &task_id, is_active))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_scheduled_task(
    project_id: String,
    task_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let pid = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let db = state
        .get_project_db(&pid)
        .ok_or_else(|| "Project DB not found".to_string())?;

    db.with_write_tx(|conn| scheduled_task_repo::delete_task(conn, &task_id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn trigger_nightly_dreaming(
    project_id: String,
    state: State<'_, AppState>,
) -> Result<trans4mers_engine::nightly_dreaming::DreamingSummary, String> {
    let pid = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    trans4mers_engine::nightly_dreaming::NightlyDreamingWorker::run_consolidation(
        state.inner(),
        &pid,
    )
    .await
    .map_err(|e| e.to_string())
}
