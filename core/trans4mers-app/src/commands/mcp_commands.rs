use std::sync::Arc;
use tauri::State;
use trans4mers_domain::ids::ProjectId;
use trans4mers_engine::app_state::AppState;
use trans4mers_engine::mcp_server_bridge::McpServerBridge;

#[tauri::command]
pub async fn handle_mcp_request(
    project_id: String,
    request_json: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    McpServerBridge::handle_request(Arc::new(state.inner().clone()), proj_id, &request_json)
        .await
        .map_err(|e| e.to_string())
}
