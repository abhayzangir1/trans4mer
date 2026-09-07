use tauri::State;
use trans4mers_domain::conversation::{Conversation, ConversationSettings, ConversationStatus};
use trans4mers_domain::ids::{ConversationId, ProjectId};
use trans4mers_engine::app_state::AppState;

#[tauri::command]
pub async fn create_conversation(
    project_id: String,
    title: String,
    state: State<'_, AppState>,
) -> Result<Conversation, String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let conv = Conversation {
        id: ConversationId::new(),
        project_id: proj_id,
        title,
        status: ConversationStatus::Active,
        settings: ConversationSettings::default(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let db = state
        .get_project_db(&proj_id)
        .ok_or_else(|| "Project DB not found".to_string())?;
    db.with_write_tx(|conn| {
        trans4mers_storage::repos::conversation_repo::insert_conversation(conn, &conv)
    })?;
    Ok(conv)
}

#[tauri::command]
pub async fn list_conversations(
    project_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<Conversation>, String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let db = state
        .get_project_db(&proj_id)
        .ok_or_else(|| "Project DB not found".to_string())?;

    let convs: Vec<Conversation> = db.with_read_conn(|conn| {
        let mut stmt = conn.prepare("SELECT id, project_id, title, status, settings, created_at, updated_at FROM conversations WHERE project_id = ?1")?;
        let iter = stmt.query_map(rusqlite::params![project_id], |row| {
            let status_str: String = row.get(3)?;
            let settings_str: String = row.get(4)?;
            let id_str: String = row.get(0)?;
            let proj_id_str: String = row.get(1)?;
            let created_str: String = row.get(5)?;
            let updated_str: String = row.get(6)?;

            let cid = ConversationId::from_str(&id_str)
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?;
            let pid = ProjectId::from_str(&proj_id_str)
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e)))?;
            let created = chrono::DateTime::parse_from_rfc3339(&created_str)
                .map(|d| d.into())
                .unwrap_or_else(|_| chrono::Utc::now());
            let updated = chrono::DateTime::parse_from_rfc3339(&updated_str)
                .map(|d| d.into())
                .unwrap_or_else(|_| chrono::Utc::now());

            Ok(Conversation {
                id: cid,
                project_id: pid,
                title: row.get(2)?,
                status: std::str::FromStr::from_str(&status_str).unwrap_or(ConversationStatus::Active),
                settings: serde_json::from_str(&settings_str).unwrap_or_default(),
                created_at: created,
                updated_at: updated,
            })
        })?;

        let mut convs = Vec::new();
        for c in iter {
            convs.push(c?);
        }
        Ok(convs)
    })?;

    Ok(convs)
}

#[tauri::command]
pub async fn get_conversation(
    project_id: String,
    conversation_id: String,
    state: State<'_, AppState>,
) -> Result<Conversation, String> {
    let proj_id = ProjectId::from_str(&project_id).map_err(|e| e.to_string())?;
    let id = ConversationId::from_str(&conversation_id).map_err(|e| e.to_string())?;
    let db = state
        .get_project_db(&proj_id)
        .ok_or_else(|| "Project DB not found".to_string())?;

    let conv = db
        .with_read_conn(|conn| {
            trans4mers_storage::repos::conversation_repo::get_conversation(conn, &id)
        })?
        .ok_or_else(|| "Conversation not found".to_string())?;

    Ok(conv)
}
