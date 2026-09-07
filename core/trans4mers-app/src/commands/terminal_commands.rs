use tauri::State;
use trans4mers_domain::ids::ProjectId;
use trans4mers_engine::app_state::AppState;
use trans4mers_storage::repos::project_repo::get_project;

#[tauri::command]
pub async fn create_terminal_session(
    project_id: String,
    cols: u16,
    rows: u16,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let proj = state
        .global_db
        .with_read_conn(|conn| get_project(conn, &proj_id))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Project not found".to_string())?;

    let session_id = uuid::Uuid::new_v4().to_string();
    let term_mgr = state.get_terminal_manager(&proj_id);
    let bus = state.global_event_bus.clone();

    tokio::task::block_in_place(|| {
        term_mgr.spawn_terminal(session_id.clone(), &proj.workspace_path, bus, cols, rows)
    })
    .map_err(|e| e.to_string())?;

    state.terminal_sessions.insert(session_id.clone(), proj_id);

    Ok(session_id)
}

#[tauri::command]
pub async fn terminal_write(
    session_id: String,
    data: Vec<u8>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if let Some(proj_id) = state.terminal_sessions.get(&session_id)
        && let Some(mgr) = state.terminal_managers.get(proj_id.value())
        && mgr
            .value()
            .write_to_terminal(&session_id, &data)
            .await
            .is_ok()
    {
        return Ok(());
    }

    // Fallback scan across all managers
    for mgr in state.terminal_managers.iter() {
        if mgr
            .value()
            .write_to_terminal(&session_id, &data)
            .await
            .is_ok()
        {
            return Ok(());
        }
    }
    Err("Session not found".to_string())
}

#[tauri::command]
pub async fn terminal_resize(
    session_id: String,
    cols: u16,
    rows: u16,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if let Some(proj_id) = state.terminal_sessions.get(&session_id)
        && let Some(mgr) = state.terminal_managers.get(proj_id.value())
        && mgr
            .value()
            .resize_terminal(&session_id, cols, rows)
            .await
            .is_ok()
    {
        return Ok(());
    }

    // Fallback scan across all managers
    for mgr in state.terminal_managers.iter() {
        if mgr
            .value()
            .resize_terminal(&session_id, cols, rows)
            .await
            .is_ok()
        {
            return Ok(());
        }
    }
    Err("Session not found".to_string())
}

#[tauri::command]
pub async fn destroy_terminal_session(
    session_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if let Some((_, proj_id)) = state.terminal_sessions.remove(&session_id)
        && let Some(mgr) = state.terminal_managers.get(&proj_id)
        && mgr.value().close_terminal(&session_id)
    {
        return Ok(());
    }

    // Fallback scan across all managers
    for mgr in state.terminal_managers.iter() {
        if mgr.value().close_terminal(&session_id) {
            return Ok(());
        }
    }
    Ok(())
}
