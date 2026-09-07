use rusqlite::{Connection, params};
use trans4mers_domain::agent::AgentMembership;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::ids::{AgentInstanceId, ConversationId};

pub fn insert_membership(
    conn: &Connection,
    membership: &AgentMembership,
) -> Result<(), Trans4mersError> {
    conn.execute(
        "INSERT INTO agent_memberships (
            agent_instance_id, conversation_id, role_in_conversation, joined_at, left_at
        ) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            membership.agent_instance_id.as_str(),
            membership.conversation_id.as_str(),
            membership.role_in_conversation,
            membership.joined_at.to_rfc3339(),
            membership.left_at.map(|d| d.to_rfc3339()),
        ],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}

pub fn list_memberships_by_agent(
    conn: &Connection,
    agent_id: &AgentInstanceId,
) -> Result<Vec<AgentMembership>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT agent_instance_id, conversation_id, role_in_conversation, joined_at, left_at
         FROM agent_memberships WHERE agent_instance_id = ?1",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let rows = stmt
        .query_map(params![agent_id.as_str()], |row| {
            Ok(AgentMembership {
                agent_instance_id: AgentInstanceId::from_str(&row.get::<_, String>(0)?)
                    .unwrap_or_default(),
                conversation_id: ConversationId::from_str(&row.get::<_, String>(1)?)
                    .unwrap_or_default(),
                role_in_conversation: row.get(2)?,
                joined_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(3)?),
                left_at: crate::datetime_util::parse_db_datetime_opt(
                    row.get::<_, Option<String>>(4)?.as_deref(),
                ),
            })
        })
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut memberships = Vec::new();
    for row in rows {
        memberships.push(row.map_err(|e| Trans4mersError::Database(e.to_string()))?);
    }
    Ok(memberships)
}

pub fn list_memberships_by_conversation(
    conn: &Connection,
    conversation_id: &ConversationId,
) -> Result<Vec<AgentMembership>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT agent_instance_id, conversation_id, role_in_conversation, joined_at, left_at
         FROM agent_memberships WHERE conversation_id = ?1",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let rows = stmt
        .query_map(params![conversation_id.as_str()], |row| {
            Ok(AgentMembership {
                agent_instance_id: AgentInstanceId::from_str(&row.get::<_, String>(0)?)
                    .unwrap_or_default(),
                conversation_id: ConversationId::from_str(&row.get::<_, String>(1)?)
                    .unwrap_or_default(),
                role_in_conversation: row.get(2)?,
                joined_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(3)?),
                left_at: crate::datetime_util::parse_db_datetime_opt(
                    row.get::<_, Option<String>>(4)?.as_deref(),
                ),
            })
        })
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut memberships = Vec::new();
    for row in rows {
        memberships.push(row.map_err(|e| Trans4mersError::Database(e.to_string()))?);
    }
    Ok(memberships)
}

pub fn delete_membership(
    conn: &Connection,
    agent_id: &AgentInstanceId,
    conversation_id: &ConversationId,
) -> Result<(), Trans4mersError> {
    conn.execute(
        "DELETE FROM agent_memberships WHERE agent_instance_id = ?1 AND conversation_id = ?2",
        params![agent_id.as_str(), conversation_id.as_str()],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}
