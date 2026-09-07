use tauri::State;
use trans4mers_domain::ids::ProjectId;
use trans4mers_domain::project::Project;
use trans4mers_engine::app_state::AppState;

#[tauri::command]
pub async fn create_project(
    name: String,
    workspace_path: String,
    description: Option<String>,
    state: State<'_, AppState>,
) -> Result<Project, String> {
    let dimensions = state.config.read().await.embedding.dimensions;
    let project = Project {
        id: ProjectId::new(),
        name,
        description,
        workspace_path: workspace_path.clone(),
        global_instructions: None,
        settings: Default::default(),
        embedding_dimensions: dimensions,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let w_path = std::path::Path::new(&workspace_path);
    if !w_path.exists() {
        std::fs::create_dir_all(w_path)
            .map_err(|e| format!("Failed to create workspace directory: {}", e))?;
    }

    // Initialize git repository physically
    trans4mers_engine::git_workspace::GitWorkspace::ensure_git_workspace(w_path)
        .map_err(|e| format!("Failed to initialize git repository: {}", e))?;

    state.global_db.with_write_tx(|conn| {
        trans4mers_storage::repos::project_repo::insert_project(conn, &project)
    })?;

    // Initialize project SQLite database and seed default "General" conversation
    let proj_db = state.get_project_db(&project.id).ok_or_else(|| {
        format!(
            "Failed to initialize project database at '{}/.trans4mers/project.sqlite'",
            workspace_path
        )
    })?;

    let conv = trans4mers_domain::conversation::Conversation {
        id: trans4mers_domain::ids::ConversationId::from_uuid(*project.id.as_uuid()),
        project_id: project.id,
        title: trans4mers_domain::constants::DEFAULT_CHANNEL_NAME.to_string(),
        status: trans4mers_domain::conversation::ConversationStatus::Active,
        settings: trans4mers_domain::conversation::ConversationSettings::default(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    proj_db
        .with_write_tx(|conn| {
            trans4mers_storage::repos::conversation_repo::insert_conversation(conn, &conv)?;

            // Seed default Boss Orchestrator agent instance into the new workspace
            let boss_agent_id = trans4mers_domain::ids::AgentInstanceId::new();
            let spawn_event = trans4mers_domain::event::DomainEvent::AgentSpawned {
                agent_id: boss_agent_id,
                project_id: project.id,
                definition_id: trans4mers_domain::constants::DEFAULT_ORCHESTRATOR_ROLE.to_string(),
                parent_id: None,
            };
            trans4mers_engine::cqrs::commit_event(conn, spawn_event, state.human_actor_id)?;
            Ok(())
        })
        .map_err(|e| {
            format!(
                "Failed to seed default workspace conversation and boss agent: {}",
                e
            )
        })?;

    Ok(project)
}

#[tauri::command]
pub async fn list_projects(state: State<'_, AppState>) -> Result<Vec<Project>, String> {
    let projects: Vec<Project> = state.global_db.with_read_conn(|conn| {
        let mut stmt = conn.prepare("SELECT id, name, description, workspace_path, global_instructions, embedding_dimensions, created_at, updated_at, settings FROM projects")?;
        let rows = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let name: String = row.get(1)?;
            let description: Option<String> = row.get(2)?;
            let workspace_path: String = row.get(3)?;
            let global_instructions: Option<String> = row.get(4)?;
            let embedding_dimensions: u32 = row.get(5)?;
            let created_at: String = row.get(6)?;
            let updated_at: String = row.get(7)?;
            let settings: String = row.get(8)?;

            let pid = ProjectId::from_str(&id)
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?;
            let created = chrono::DateTime::parse_from_rfc3339(&created_at)
                .map(|d| d.into())
                .unwrap_or_else(|_| chrono::Utc::now());
            let updated = chrono::DateTime::parse_from_rfc3339(&updated_at)
                .map(|d| d.into())
                .unwrap_or_else(|_| chrono::Utc::now());

            Ok(Project {
                id: pid,
                name,
                description,
                workspace_path,
                global_instructions,
                embedding_dimensions,
                created_at: created,
                updated_at: updated,
                settings: serde_json::from_str(&settings).unwrap_or_default(),
            })
        })?;

        let mut projects = Vec::new();
        for p in rows.flatten() { projects.push(p); }
        Ok(projects)
    })?;

    Ok(projects)
}

#[tauri::command]
pub async fn get_project(
    project_id: String,
    state: State<'_, AppState>,
) -> Result<Project, String> {
    let id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let proj = state
        .global_db
        .with_read_conn(|conn| trans4mers_storage::repos::project_repo::get_project(conn, &id))?
        .ok_or_else(|| "Project not found".to_string())?;

    Ok(proj)
}

#[tauri::command]
pub async fn delete_project(project_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    state.global_db.with_write_tx(|conn| {
        conn.execute("DELETE FROM projects WHERE id = ?1", [id.as_str()])?;
        Ok(())
    })?;
    state.project_dbs.remove(&id);
    state.tool_executors.remove(&id);
    Ok(())
}

#[tauri::command]
pub async fn vector_migrate(
    project_id: String,
    target_backend: String,
    state: State<'_, AppState>,
) -> Result<trans4mers_storage::VectorMigrationReport, String> {
    let id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let proj = state
        .global_db
        .with_read_conn(|conn| trans4mers_storage::repos::project_repo::get_project(conn, &id))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Project '{}' not found", project_id))?;

    let proj_db = state
        .get_project_db(&id)
        .ok_or_else(|| format!("Failed to load database for project '{}'", project_id))?;

    let current_backend = proj_db
        .with_read_conn(trans4mers_storage::repos::settings_repo::get_vector_backend)
        .unwrap_or_else(|_| "sqlite-vec".to_string());

    let workspace_path = std::path::PathBuf::from(proj.workspace_path);
    let base_lancedb_dir = workspace_path.join(".trans4mers").join("lancedb");

    let report = trans4mers_storage::migrate_vector_backend(
        &proj_db,
        &project_id,
        &base_lancedb_dir,
        &current_backend,
        &target_backend,
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(report)
}

#[tauri::command]
pub async fn get_vector_backend(
    project_id: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let proj_db = state
        .get_project_db(&id)
        .ok_or_else(|| format!("Failed to load database for project '{}'", project_id))?;

    let backend = proj_db
        .with_read_conn(trans4mers_storage::repos::settings_repo::get_vector_backend)
        .map_err(|e| e.to_string())?;

    Ok(backend)
}
