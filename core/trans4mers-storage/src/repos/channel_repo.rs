use rusqlite::{Connection, OptionalExtension, params};
use std::str::FromStr;
use trans4mers_domain::channel::{Channel, ChannelKind};
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::ids::{ChannelId, ConversationId, ProjectId};

pub fn insert_channel(conn: &Connection, channel: &Channel) -> Result<(), Trans4mersError> {
    let member_actors_json =
        serde_json::to_string(&channel.member_actors).unwrap_or_else(|_| "[]".to_string());
    conn.execute(
        "INSERT INTO channels (
            id, conversation_id, name, kind, is_read_only, member_actors, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            channel.id.as_str(),
            channel.conversation_id.as_str(),
            channel.name,
            channel.kind.to_string(),
            channel.is_read_only as i32,
            member_actors_json,
            channel.created_at.to_rfc3339(),
            channel.updated_at.to_rfc3339(),
        ],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}

pub fn get_channel(conn: &Connection, id: &ChannelId) -> Result<Option<Channel>, Trans4mersError> {
    let mut stmt = conn.prepare(
        "SELECT id, conversation_id, name, kind, is_read_only, member_actors, created_at, updated_at
         FROM channels WHERE id = ?1"
    ).map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let channel = stmt
        .query_row(params![id.as_str()], |row| {
            let kind_str: String = row.get(3)?;
            let is_read_only: i32 = row.get(4)?;
            let members_str: String = row.get(5)?;

            Ok(Channel {
                id: ChannelId::from_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                conversation_id: ConversationId::from_str(&row.get::<_, String>(1)?)
                    .unwrap_or_default(),
                name: row.get(2)?,
                kind: ChannelKind::from_str(&kind_str).unwrap_or(ChannelKind::General),
                is_read_only: is_read_only != 0,
                member_actors: serde_json::from_str(&members_str).unwrap_or_default(),
                created_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(6)?),
                updated_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(7)?),
            })
        })
        .optional()
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    Ok(channel)
}

pub fn list_channels_by_project(
    conn: &Connection,
    project_id: &ProjectId,
) -> Result<Vec<Channel>, Trans4mersError> {
    let mut stmt = conn.prepare(
        "SELECT ch.id, ch.conversation_id, ch.name, ch.kind, ch.is_read_only, ch.member_actors, ch.created_at, ch.updated_at
         FROM channels ch
         JOIN conversations c ON ch.conversation_id = c.id
         WHERE c.project_id = ?1"
    ).map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let rows = stmt
        .query_map(params![project_id.as_str()], |row| {
            let kind_str: String = row.get(3)?;
            let is_read_only: i32 = row.get(4)?;
            let members_str: String = row.get(5)?;

            Ok(Channel {
                id: ChannelId::from_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                conversation_id: ConversationId::from_str(&row.get::<_, String>(1)?)
                    .unwrap_or_default(),
                name: row.get(2)?,
                kind: ChannelKind::from_str(&kind_str).unwrap_or(ChannelKind::General),
                is_read_only: is_read_only != 0,
                member_actors: serde_json::from_str(&members_str).unwrap_or_default(),
                created_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(6)?),
                updated_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(7)?),
            })
        })
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut channels = Vec::new();
    for row in rows {
        channels.push(row.map_err(|e| Trans4mersError::Database(e.to_string()))?);
    }
    Ok(channels)
}
