use rusqlite::{Connection, OptionalExtension, params};
use serde_json;
use trans4mers_domain::agent::AgentInstance;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::ids::AgentInstanceId;

pub fn insert_agent_instance(
    conn: &Connection,
    agent: &AgentInstance,
) -> Result<(), Trans4mersError> {
    let capabilities_json = serde_json::to_string(&agent.capabilities)
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    let model_override_json = agent
        .model_config_override
        .as_ref()
        .map(|m| serde_json::to_string(m).unwrap());

    conn.execute(
        "INSERT INTO agent_instances (
            id, project_id, definition_id, parent_instance_id, status,
            capabilities, model_config_override, depth_level, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        ON CONFLICT(id) DO NOTHING",
        params![
            agent.id.as_str(),
            agent.project_id.as_str(),
            agent.definition_id.as_str(),
            agent.parent_instance_id.as_ref().map(|id| id.as_str()),
            agent.status.to_string(),
            capabilities_json,
            model_override_json,
            agent.depth_level,
            agent.created_at.to_rfc3339(),
            agent.updated_at.to_rfc3339(),
        ],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    Ok(())
}

pub fn get_agent_instance(
    conn: &Connection,
    id: &AgentInstanceId,
) -> Result<Option<AgentInstance>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT
            id, project_id, definition_id, parent_instance_id, status,
            capabilities, model_config_override, depth_level, created_at, updated_at
         FROM agent_instances WHERE id = ?1",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let agent = stmt
        .query_row(params![id.as_str()], |row| {
            let caps_str: String = row.get(5)?;
            let capabilities = serde_json::from_str(&caps_str)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

            let model_override_str: Option<String> = row.get(6)?;
            let model_config_override =
                model_override_str.and_then(|s| serde_json::from_str(&s).ok());

            let status_str: String = row.get(4)?;
            let status = std::str::FromStr::from_str(&status_str)
                .map_err(|_| rusqlite::Error::InvalidQuery)?;

            let parent_id_str: Option<String> = row.get(3)?;
            let parent_instance_id =
                parent_id_str.and_then(|s| std::str::FromStr::from_str(&s).ok());

            Ok(AgentInstance {
                id: std::str::FromStr::from_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                project_id: std::str::FromStr::from_str(&row.get::<_, String>(1)?)
                    .unwrap_or_default(),
                definition_id: std::str::FromStr::from_str(&row.get::<_, String>(2)?)
                    .unwrap_or_default(),
                parent_instance_id,
                status,
                capabilities,
                model_config_override,
                depth_level: row.get(7)?,
                created_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(8)?),
                updated_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(9)?),
            })
        })
        .optional()
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    Ok(agent)
}

pub fn list_agent_instances(
    conn: &Connection,
    project_id: &trans4mers_domain::ids::ProjectId,
) -> Result<Vec<AgentInstance>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT
            id, project_id, definition_id, parent_instance_id, status,
            capabilities, model_config_override, depth_level, created_at, updated_at
         FROM agent_instances WHERE project_id = ?1",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let rows = stmt
        .query_map(params![project_id.as_str()], |row| {
            let caps_str: String = row.get(5)?;
            let capabilities = serde_json::from_str(&caps_str)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

            let model_override_str: Option<String> = row.get(6)?;
            let model_config_override =
                model_override_str.and_then(|s| serde_json::from_str(&s).ok());

            let status_str: String = row.get(4)?;
            let status = std::str::FromStr::from_str(&status_str)
                .map_err(|_| rusqlite::Error::InvalidQuery)?;

            let parent_id_str: Option<String> = row.get(3)?;
            let parent_instance_id =
                parent_id_str.and_then(|s| std::str::FromStr::from_str(&s).ok());

            Ok(AgentInstance {
                id: std::str::FromStr::from_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                project_id: std::str::FromStr::from_str(&row.get::<_, String>(1)?)
                    .unwrap_or_default(),
                definition_id: std::str::FromStr::from_str(&row.get::<_, String>(2)?)
                    .unwrap_or_default(),
                parent_instance_id,
                status,
                capabilities,
                model_config_override,
                depth_level: row.get(7)?,
                created_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(8)?),
                updated_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(9)?),
            })
        })
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut agents = Vec::new();
    for row in rows {
        agents.push(row.map_err(|e| Trans4mersError::Database(e.to_string()))?);
    }
    Ok(agents)
}

pub fn update_agent_instance_status(
    conn: &Connection,
    id: &AgentInstanceId,
    status: trans4mers_domain::state::AgentStatus,
) -> Result<(), Trans4mersError> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE agent_instances SET status = ?1, updated_at = ?2 WHERE id = ?3",
        params![status.to_string(), now, id.as_str()],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}
