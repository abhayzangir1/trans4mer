use tauri::State;
use trans4mers_domain::ids::ProjectId;
use trans4mers_engine::app_state::AppState;

#[tauri::command]
pub async fn get_git_status(
    project_id: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let proj = state
        .global_db
        .with_read_conn(|conn| trans4mers_storage::repos::project_repo::get_project(conn, &proj_id))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Project not found".to_string())?;

    tokio::task::block_in_place(|| {
        let repo = git2::Repository::open(&proj.workspace_path).map_err(|e| e.to_string())?;
        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(true);
        let statuses = repo.statuses(Some(&mut opts)).map_err(|e| e.to_string())?;
        if statuses.is_empty() {
            Ok("Clean".to_string())
        } else {
            Ok(format!("{} files changed", statuses.len()))
        }
    })
}
