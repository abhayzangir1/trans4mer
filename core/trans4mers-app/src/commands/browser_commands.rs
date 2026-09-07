use std::sync::Arc;
use tauri::State;
use trans4mers_domain::browser::{BrowserSnapshot, BrowserSpace};
use trans4mers_domain::ids::ProjectId;
use trans4mers_engine::app_state::AppState;
use trans4mers_engine::browser_space_manager::{BrowserLiveFrame, BrowserSpaceManager};

#[tauri::command]
pub async fn get_browser_spaces(
    project_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<BrowserSpace>, String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    BrowserSpaceManager::list_browser_spaces(Arc::new(state.inner().clone()), &proj_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_browser_space(
    project_id: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<BrowserSpace, String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    BrowserSpaceManager::create_browser_space(Arc::new(state.inner().clone()), proj_id, name)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn browser_navigate(
    project_id: String,
    space_id: String,
    url: String,
    state: State<'_, AppState>,
) -> Result<BrowserLiveFrame, String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    BrowserSpaceManager::navigate(Arc::new(state.inner().clone()), &proj_id, &space_id, &url)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_browser_snapshots(
    project_id: String,
    space_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<BrowserSnapshot>, String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    BrowserSpaceManager::list_snapshots(Arc::new(state.inner().clone()), &proj_id, &space_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn browser_rollback_snapshot(
    project_id: String,
    space_id: String,
    snapshot_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    BrowserSpaceManager::rollback_to_snapshot(
        Arc::new(state.inner().clone()),
        &proj_id,
        &space_id,
        &snapshot_id,
    )
    .await
    .map_err(|e| e.to_string())
}
