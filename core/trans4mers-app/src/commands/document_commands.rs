use tauri::State;
use trans4mers_domain::document::{DocIngestState, DocSearchResult};
use trans4mers_domain::ids::ProjectId;
use trans4mers_engine::app_state::AppState;
use trans4mers_engine::document_rag::DocumentRagEngine;

#[tauri::command]
pub async fn ingest_documents(
    project_id: String,
    paths: Option<Vec<String>>,
    state: State<'_, AppState>,
) -> Result<usize, String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    DocumentRagEngine::ingest_project(&state, &proj_id, paths)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn search_documents(
    project_id: String,
    query: String,
    limit: Option<usize>,
    file_pattern: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<DocSearchResult>, String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    DocumentRagEngine::search(
        &state,
        &proj_id,
        &query,
        limit.unwrap_or(10),
        file_pattern.as_deref(),
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_ingested_documents(
    project_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<DocIngestState>, String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    DocumentRagEngine::list_ingested_files(&state, &proj_id).map_err(|e| e.to_string())
}
