use tauri::State;
use trans4mers_domain::ids::ProjectId;
use trans4mers_engine::app_state::AppState;
use trans4mers_storage::repos::project_repo::get_project;

#[tauri::command]
pub async fn read_file(
    project_id: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let proj = state
        .global_db
        .with_read_conn(|conn| get_project(conn, &proj_id))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Project not found".to_string())?;

    let root = std::path::PathBuf::from(proj.workspace_path);
    let rel_path = if let Ok(stripped) = std::path::Path::new(&path).strip_prefix(&root) {
        stripped.to_str().unwrap_or(&path)
    } else {
        &path
    };
    tokio::task::block_in_place(|| {
        trans4mers_storage::filesystem::FileSystemGuard::read_workspace_file(&root, rel_path)
    })
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn write_file(
    project_id: String,
    path: String,
    content: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let proj = state
        .global_db
        .with_read_conn(|conn| get_project(conn, &proj_id))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Project not found".to_string())?;

    let root = std::path::PathBuf::from(proj.workspace_path);
    let rel_path = if let Ok(stripped) = std::path::Path::new(&path).strip_prefix(&root) {
        stripped.to_str().unwrap_or(&path).to_string()
    } else {
        path.clone()
    };
    tokio::task::block_in_place(|| {
        trans4mers_storage::filesystem::FileSystemGuard::write_workspace_file(
            &root, &rel_path, &content,
        )
    })
    .map_err(|e| e.to_string())?;

    // Bi-Directional Human Sync: Emit FileModifiedByHuman event
    let event = trans4mers_domain::event::DomainEvent::FileModifiedByHuman {
        project_id: proj_id,
        relative_path: rel_path.clone(),
        diff_summary: format!(
            "User saved manual changes to `{}` ({} bytes)",
            rel_path,
            content.len()
        ),
    };

    if let Some(db) = state.get_project_db(&proj_id)
        && let Ok(envelope) = db.with_write_tx(|conn| {
            trans4mers_engine::cqrs::commit_event(conn, event, state.human_actor_id)
        })
    {
        let env_arc = std::sync::Arc::new(envelope);
        let bus = state.get_event_bus(&proj_id);
        let _ = bus.publish(env_arc.clone());
        let _ = state.global_event_bus.publish(env_arc);
    }

    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

#[tauri::command]
pub async fn list_directory(
    project_id: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<Vec<FileEntry>, String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let proj = state
        .global_db
        .with_read_conn(|conn| get_project(conn, &proj_id))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Project not found".to_string())?;

    let root = std::path::PathBuf::from(proj.workspace_path);

    // Convert logic to safely list directory contents within the workspace root
    tokio::task::block_in_place(|| {
        let abs_path = if path.is_empty() || path == "." {
            root.clone()
        } else if let Ok(stripped) = std::path::Path::new(&path).strip_prefix(&root) {
            root.join(stripped)
        } else {
            root.join(&path)
        };

        let validated_path =
            trans4mers_storage::filesystem::FileSystemGuard::validate_path_in_workspace(
                &root, &abs_path,
            )
            .map_err(|e| e.to_string())?;

        let mut entries = Vec::new();
        if let Ok(dir) = std::fs::read_dir(&validated_path) {
            for entry in dir.flatten() {
                if let Ok(name) = entry.file_name().into_string() {
                    let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
                    let rel_path = if path.is_empty() {
                        name.clone()
                    } else {
                        format!("{}/{}", path, name)
                    };
                    entries.push(FileEntry {
                        name,
                        path: rel_path,
                        is_dir,
                    });
                }
            }
        }
        Ok(entries)
    })
}

#[tauri::command]
pub async fn pick_directory() -> Result<Option<String>, String> {
    let folder = rfd::AsyncFileDialog::new()
        .set_title("Select Workspace Directory")
        .pick_folder()
        .await;
    Ok(folder.map(|f| f.path().to_string_lossy().to_string()))
}
