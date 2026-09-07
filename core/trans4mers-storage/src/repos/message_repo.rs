use rusqlite::{Connection, OptionalExtension, params};
use serde_json;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::ids::MessageId;
use trans4mers_domain::message::Message;

pub fn insert_message(conn: &Connection, msg: &Message) -> Result<(), Trans4mersError> {
    let sender_json = serde_json::to_string(&msg.sender)
        .map_err(|e| Trans4mersError::Database(format!("Failed to serialize sender: {}", e)))?;
    let mentions_json = serde_json::to_string(&msg.mentions)
        .map_err(|e| Trans4mersError::Database(format!("Failed to serialize mentions: {}", e)))?;
    let attachments_json = serde_json::to_string(&msg.attachments)
        .map_err(|e| Trans4mersError::Database(format!("Failed to serialize attachments: {}", e)))?;

    conn.execute(
        "INSERT INTO messages (
            id, conversation_id, channel_id, thread_id, sender_actor,
            content, message_kind, mentions, attachments,
            requires_approval, approval_id, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            msg.id.as_str(),
            msg.conversation_id.as_str(),
            msg.channel_id.as_str(),
            msg.thread_id.as_ref().map(|id| id.as_str()),
            sender_json,
            msg.content,
            msg.message_kind.to_string(),
            mentions_json,
            attachments_json,
            msg.requires_approval,
            msg.approval_id.as_ref().map(|id| id.as_str()),
            msg.created_at.to_rfc3339(),
            msg.updated_at.to_rfc3339(),
        ],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    Ok(())
}

pub fn get_message(conn: &Connection, id: &MessageId) -> Result<Option<Message>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT
            id, conversation_id, channel_id, thread_id, sender_actor,
            content, message_kind, mentions, attachments,
            requires_approval, approval_id, created_at, updated_at
         FROM messages WHERE id = ?1",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    let msg = stmt
        .query_row(params![id.as_str()], |row| {
            let sender_type_str: String = row.get(4)?;
            let sender = serde_json::from_str(&sender_type_str).unwrap_or_else(|_| {
                trans4mers_domain::actor::Actor::Human {
                    id: trans4mers_domain::ids::ActorId::new(),
                    display_name: "Human".to_string(),
                }
            });

            let mentions_str: String = row.get(7)?;
            let mentions = serde_json::from_str(&mentions_str).unwrap_or_default();

            let attachments_str: String = row.get(8)?;
            let attachments = serde_json::from_str(&attachments_str).unwrap_or_default();

            let thread_id_str: Option<String> = row.get(3)?;
            let thread_id = thread_id_str.and_then(|s| std::str::FromStr::from_str(&s).ok());

            let approval_id_str: Option<String> = row.get(10)?;
            let approval_id = approval_id_str.and_then(|s| std::str::FromStr::from_str(&s).ok());

            Ok(Message {
                id: std::str::FromStr::from_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                conversation_id: std::str::FromStr::from_str(&row.get::<_, String>(1)?)
                    .unwrap_or_default(),
                channel_id: std::str::FromStr::from_str(&row.get::<_, String>(2)?)
                    .unwrap_or_default(),
                thread_id,
                sender,
                content: row.get(5)?,
                message_kind: std::str::FromStr::from_str(&row.get::<_, String>(6)?)
                    .unwrap_or(trans4mers_domain::message::MessageKind::Chat),
                mentions,
                attachments,
                requires_approval: row.get(9)?,
                approval_id,
                created_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(11)?),
                updated_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(12)?),
            })
        })
        .optional()
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    Ok(msg)
}

pub fn list_messages(
    conn: &Connection,
    conversation_id: &str,
    channel_id: &str,
    limit: u32,
    offset: u32,
) -> Result<Vec<Message>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT
            id, conversation_id, channel_id, thread_id, sender_actor,
            content, message_kind, mentions, attachments, requires_approval,
            approval_id, created_at, updated_at
         FROM messages
         WHERE (?1 = '' OR conversation_id = ?1) AND (?2 = '' OR ?2 = 'all' OR channel_id = ?2)
         ORDER BY created_at ASC
         LIMIT ?3 OFFSET ?4",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let iter = stmt
        .query_map(params![conversation_id, channel_id, limit, offset], |row| {
            let sender_json: String = row.get(4)?;
            let mentions_json: String = row.get(7)?;
            let attachments_json: String = row.get(8)?;

            let thread_id_str: Option<String> = row.get(3)?;
            let thread_id = thread_id_str.and_then(|s| std::str::FromStr::from_str(&s).ok());

            let approval_id_str: Option<String> = row.get(10)?;
            let approval_id = approval_id_str.and_then(|s| std::str::FromStr::from_str(&s).ok());

            Ok(Message {
                id: std::str::FromStr::from_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                conversation_id: std::str::FromStr::from_str(&row.get::<_, String>(1)?)
                    .unwrap_or_default(),
                channel_id: std::str::FromStr::from_str(&row.get::<_, String>(2)?)
                    .unwrap_or_default(),
                thread_id,
                sender: serde_json::from_str(&sender_json).unwrap_or_else(|_| {
                    trans4mers_domain::actor::Actor::Human {
                        id: trans4mers_domain::ids::ActorId::new(),
                        display_name: "Human".to_string(),
                    }
                }),
                content: row.get(5)?,
                message_kind: std::str::FromStr::from_str(&row.get::<_, String>(6)?)
                    .unwrap_or(trans4mers_domain::message::MessageKind::Chat),
                mentions: serde_json::from_str(&mentions_json).unwrap_or_default(),
                attachments: serde_json::from_str(&attachments_json).unwrap_or_default(),
                requires_approval: row.get(9)?,
                approval_id,
                created_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(11)?),
                updated_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(12)?),
            })
        })
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut msgs = Vec::new();
    for m in iter {
        msgs.push(m.map_err(|e| Trans4mersError::Database(e.to_string()))?);
    }

    Ok(msgs)
}
