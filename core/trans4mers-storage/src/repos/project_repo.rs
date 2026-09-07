use rusqlite::{Connection, OptionalExtension, params};
use serde_json;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::ids::ProjectId;
use trans4mers_domain::project::Project;

pub fn insert_project(conn: &Connection, project: &Project) -> Result<(), Trans4mersError> {
    let settings_json = serde_json::to_string(&project.settings)
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let global_instructions = project.global_instructions.as_deref().unwrap_or("");

    conn.execute(
        "INSERT INTO projects (
            id, name, workspace_path, description, global_instructions,
            settings, embedding_dimensions, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            project.id.as_str(),
            project.name,
            project.workspace_path,
            project.description,
            global_instructions,
            settings_json,
            project.embedding_dimensions,
            project.created_at.to_rfc3339(),
            project.updated_at.to_rfc3339(),
        ],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    Ok(())
}

pub fn get_project(conn: &Connection, id: &ProjectId) -> Result<Option<Project>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT
            id, name, workspace_path, description, global_instructions,
            settings, embedding_dimensions, created_at, updated_at
         FROM projects WHERE id = ?1",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let project = stmt
        .query_row(params![id.as_str()], |row| {
            let settings_str: String = row.get(5)?;
            let settings = serde_json::from_str(&settings_str).unwrap_or_default();

            Ok(Project {
                id: std::str::FromStr::from_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                name: row.get(1)?,
                workspace_path: row.get(2)?,
                description: row.get(3)?,
                global_instructions: row.get(4)?,
                settings,
                embedding_dimensions: row.get(6)?,
                created_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(7)?),
                updated_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(8)?),
            })
        })
        .optional()
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    Ok(project)
}

pub fn list_projects(conn: &Connection) -> Result<Vec<Project>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT
            id, name, workspace_path, description, global_instructions,
            settings, embedding_dimensions, created_at, updated_at
         FROM projects ORDER BY created_at DESC",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let rows = stmt
        .query_map([], |row| {
            let settings_str: String = row.get(5)?;
            let settings = serde_json::from_str(&settings_str).unwrap_or_default();

            Ok(Project {
                id: std::str::FromStr::from_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                name: row.get(1)?,
                workspace_path: row.get(2)?,
                description: row.get(3)?,
                global_instructions: row.get(4)?,
                settings,
                embedding_dimensions: row.get(6)?,
                created_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(7)?),
                updated_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(8)?),
            })
        })
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut list = Vec::new();
    for p in rows.flatten() {
        list.push(p);
    }

    Ok(list)
}
