use rusqlite::{Connection, OptionalExtension, params};
use trans4mers_domain::conversation::{Conversation, ConversationStatus};
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::ids::ConversationId;

pub fn insert_conversation(conn: &Connection, conv: &Conversation) -> Result<(), Trans4mersError> {
    let settings_json = serde_json::to_string(&conv.settings).unwrap_or_else(|_| "{}".to_string());
    conn.execute(
        "INSERT INTO conversations (
            id, project_id, title, status, settings, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            conv.id.as_str(),
            conv.project_id.as_str(),
            conv.title,
            conv.status.to_string(),
            settings_json,
            conv.created_at.to_rfc3339(),
            conv.updated_at.to_rfc3339(),
        ],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    Ok(())
}

pub fn get_conversation(
    conn: &Connection,
    id: &ConversationId,
) -> Result<Option<Conversation>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, title, status, settings, created_at, updated_at
         FROM conversations WHERE id = ?1",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let conv = stmt
        .query_row(params![id.as_str()], |row| {
            let status_str: String = row.get(3)?;
            let settings_str: String = row.get(4)?;
            Ok(Conversation {
                id: std::str::FromStr::from_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                project_id: std::str::FromStr::from_str(&row.get::<_, String>(1)?)
                    .unwrap_or_default(),
                title: row.get(2)?,
                status: std::str::FromStr::from_str(&status_str)
                    .unwrap_or(ConversationStatus::Active),
                settings: serde_json::from_str(&settings_str).unwrap_or_default(),
                created_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(5)?),
                updated_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(6)?),
            })
        })
        .optional()
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    Ok(conv)
}

pub fn list_conversations(
    conn: &Connection,
    project_id: &trans4mers_domain::ids::ProjectId,
) -> Result<Vec<Conversation>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, title, status, settings, created_at, updated_at
         FROM conversations WHERE project_id = ?1",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let rows = stmt
        .query_map(params![project_id.as_str()], |row| {
            let status_str: String = row.get(3)?;
            let settings_str: String = row.get(4)?;
            Ok(Conversation {
                id: std::str::FromStr::from_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                project_id: std::str::FromStr::from_str(&row.get::<_, String>(1)?)
                    .unwrap_or_default(),
                title: row.get(2)?,
                status: std::str::FromStr::from_str(&status_str)
                    .unwrap_or(ConversationStatus::Active),
                settings: serde_json::from_str(&settings_str).unwrap_or_default(),
                created_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(5)?),
                updated_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(6)?),
            })
        })
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut convs = Vec::new();
    for row in rows {
        convs.push(row.map_err(|e| Trans4mersError::Database(e.to_string()))?);
    }
    Ok(convs)
}
