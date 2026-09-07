use tauri::State;
use trans4mers_domain::ids::{ConversationId, ProjectId, WorkflowId, WorkflowRunId};
use trans4mers_domain::workflow::Workflow;
use trans4mers_engine::app_state::AppState;

#[tauri::command]
pub async fn create_workflow(
    project_id: String,
    conversation_id: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<Workflow, String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let convo_id = ConversationId::from_str(&conversation_id).map_err(|e| e.to_string())?;
    let wf = Workflow {
        id: WorkflowId::new(),
        conversation_id: convo_id,
        name,
        description: "".to_string(),
        nodes: vec![],
        edges: vec![],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let db = state
        .get_project_db(&proj_id)
        .ok_or_else(|| "Project DB not found".to_string())?;
    db.with_write_tx(|conn| trans4mers_storage::repos::workflow_repo::insert_workflow(conn, &wf))?;

    Ok(wf)
}

#[tauri::command]
pub async fn start_workflow_run(
    project_id: String,
    workflow_id: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let wf_id = WorkflowId::from_str(&workflow_id).map_err(|e| e.to_string())?;

    let engine = trans4mers_engine::WorkflowEngine::new(std::sync::Arc::new(state.inner().clone()));
    let run_id = WorkflowRunId::new();

    engine.execute_workflow_run(proj_id, wf_id, run_id).await;

    Ok(run_id.to_string())
}
