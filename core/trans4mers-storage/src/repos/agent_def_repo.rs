use rusqlite::{Connection, OptionalExtension, params};
use serde_json;
use trans4mers_domain::agent::AgentDefinition;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::ids::AgentDefinitionId;

pub fn insert_agent_definition(
    conn: &Connection,
    def: &AgentDefinition,
) -> Result<(), Trans4mersError> {
    let baseline_capabilities = serde_json::to_string(&def.baseline_capabilities)
        .map_err(|e| Trans4mersError::Serialization(e.to_string()))?;
    let default_skills = serde_json::to_string(&def.default_skills)
        .map_err(|e| Trans4mersError::Serialization(e.to_string()))?;
    let default_tools = serde_json::to_string(&def.default_tools)
        .map_err(|e| Trans4mersError::Serialization(e.to_string()))?;
    let metadata = serde_json::to_string(&def.metadata)
        .map_err(|e| Trans4mersError::Serialization(e.to_string()))?;
    let default_model_config = serde_json::to_string(&def.default_model_config)
        .map_err(|e| Trans4mersError::Serialization(e.to_string()))?;

    conn.execute(
        "INSERT INTO agent_definitions (
            id, name, role, description, system_instructions,
            default_model_config, baseline_capabilities, default_skills,
            default_tools, metadata, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            def.id.as_str(),
            def.name,
            def.role,
            def.description,
            def.system_instructions,
            default_model_config,
            baseline_capabilities,
            default_skills,
            default_tools,
            metadata,
            def.created_at.to_rfc3339(),
            def.updated_at.to_rfc3339(),
        ],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    Ok(())
}

pub fn update_agent_definition(
    conn: &Connection,
    def: &AgentDefinition,
) -> Result<(), Trans4mersError> {
    let baseline_capabilities = serde_json::to_string(&def.baseline_capabilities)
        .map_err(|e| Trans4mersError::Serialization(e.to_string()))?;
    let default_skills = serde_json::to_string(&def.default_skills)
        .map_err(|e| Trans4mersError::Serialization(e.to_string()))?;
    let default_tools = serde_json::to_string(&def.default_tools)
        .map_err(|e| Trans4mersError::Serialization(e.to_string()))?;
    let metadata = serde_json::to_string(&def.metadata)
        .map_err(|e| Trans4mersError::Serialization(e.to_string()))?;
    let default_model_config = serde_json::to_string(&def.default_model_config)
        .map_err(|e| Trans4mersError::Serialization(e.to_string()))?;

    conn.execute(
        "UPDATE agent_definitions SET
            name = ?1, role = ?2, description = ?3, system_instructions = ?4,
            default_model_config = ?5, baseline_capabilities = ?6, default_skills = ?7,
            default_tools = ?8, metadata = ?9, updated_at = ?10
         WHERE id = ?11",
        params![
            def.name,
            def.role,
            def.description,
            def.system_instructions,
            default_model_config,
            baseline_capabilities,
            default_skills,
            default_tools,
            metadata,
            chrono::Utc::now().to_rfc3339(),
            def.id.as_str(),
        ],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    Ok(())
}

pub fn get_agent_definition(
    conn: &Connection,
    id: &AgentDefinitionId,
) -> Result<Option<AgentDefinition>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT
            id, name, role, description, system_instructions,
            default_model_config, baseline_capabilities, default_skills,
            default_tools, metadata, created_at, updated_at
         FROM agent_definitions WHERE id = ?1",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let def = stmt
        .query_row(params![id.as_str()], |row| {
            let default_model_config: trans4mers_domain::config::ModelConfig =
                serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or_default();
            let baseline_capabilities: Vec<trans4mers_domain::tool::Capability> =
                serde_json::from_str(&row.get::<_, String>(6)?).unwrap_or_default();
            let default_skills: Vec<String> =
                serde_json::from_str(&row.get::<_, String>(7)?).unwrap_or_default();
            let default_tools: Vec<String> =
                serde_json::from_str(&row.get::<_, String>(8)?).unwrap_or_default();
            let metadata: std::collections::HashMap<String, String> =
                serde_json::from_str(&row.get::<_, String>(9)?).unwrap_or_default();

            Ok(AgentDefinition {
                id: std::str::FromStr::from_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                name: row.get(1)?,
                role: row.get(2)?,
                description: row.get(3)?,
                system_instructions: row.get(4)?,
                default_model_config,
                baseline_capabilities,
                default_skills,
                default_tools,
                metadata,
                created_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(10)?),
                updated_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(11)?),
            })
        })
        .optional()
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    Ok(def)
}

pub fn list_agent_definitions(conn: &Connection) -> Result<Vec<AgentDefinition>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT
            id, name, role, description, system_instructions,
            default_model_config, baseline_capabilities, default_skills,
            default_tools, metadata, created_at, updated_at
         FROM agent_definitions ORDER BY name ASC",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let rows = stmt
        .query_map([], |row| {
            let default_model_config: trans4mers_domain::config::ModelConfig =
                serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or_default();
            let baseline_capabilities: Vec<trans4mers_domain::tool::Capability> =
                serde_json::from_str(&row.get::<_, String>(6)?).unwrap_or_default();
            let default_skills: Vec<String> =
                serde_json::from_str(&row.get::<_, String>(7)?).unwrap_or_default();
            let default_tools: Vec<String> =
                serde_json::from_str(&row.get::<_, String>(8)?).unwrap_or_default();
            let metadata: std::collections::HashMap<String, String> =
                serde_json::from_str(&row.get::<_, String>(9)?).unwrap_or_default();

            Ok(AgentDefinition {
                id: std::str::FromStr::from_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                name: row.get(1)?,
                role: row.get(2)?,
                description: row.get(3)?,
                system_instructions: row.get(4)?,
                default_model_config,
                baseline_capabilities,
                default_skills,
                default_tools,
                metadata,
                created_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(10)?),
                updated_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(11)?),
            })
        })
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut result = Vec::new();
    for r in rows {
        result.push(r.map_err(|e| Trans4mersError::Database(e.to_string()))?);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_agent_definition_with_sqlite_current_timestamp() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("
            CREATE TABLE agent_definitions (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                role TEXT NOT NULL,
                description TEXT NOT NULL,
                system_instructions TEXT NOT NULL,
                default_model_config TEXT NOT NULL,
                baseline_capabilities TEXT NOT NULL,
                default_skills TEXT NOT NULL,
                default_tools TEXT NOT NULL,
                metadata TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );
            INSERT INTO agent_definitions (id, name, role, description, system_instructions, default_model_config, baseline_capabilities, default_skills, default_tools, metadata)
            VALUES ('boss', 'Boss Agent', 'Orchestrator', 'Project manager', 'You are the orchestrator.', '{\"provider\":\"ollama\",\"model\":\"qwen2.5-coder:3b\",\"temperature\":0.2}', '[]', '[]', '[]', '{}');
        ").unwrap();

        let id = AgentDefinitionId::from_str("boss").unwrap();
        let def = get_agent_definition(&conn, &id).unwrap();
        assert!(def.is_some());
        let def = def.unwrap();
        assert_eq!(def.name, "Boss Agent");
        assert_eq!(def.role, "Orchestrator");

        let list = list_agent_definitions(&conn).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id.as_str(), "boss");
    }
}
