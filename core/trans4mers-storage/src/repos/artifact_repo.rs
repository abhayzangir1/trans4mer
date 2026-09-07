use rusqlite::{Connection, OptionalExtension, params};
use trans4mers_domain::artifact::Artifact;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::ids::ArtifactId;

pub fn insert_artifact(conn: &Connection, artifact: &Artifact) -> Result<(), Trans4mersError> {
    conn.execute(
        "INSERT INTO artifacts (
            id, project_id, producer_execution_id, relative_path,
            content_hash, mime_type, size_bytes, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            artifact.id.as_str(),
            artifact.project_id.as_str(),
            artifact
                .producer_execution_id
                .as_ref()
                .map(|id| id.as_str()),
            artifact.relative_path,
            artifact.content_hash,
            artifact.mime_type,
            artifact.size_bytes,
            artifact.created_at.to_rfc3339(),
        ],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}

pub fn get_artifact(
    conn: &Connection,
    id: &ArtifactId,
) -> Result<Option<Artifact>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT
            id, project_id, producer_execution_id, relative_path,
            content_hash, mime_type, size_bytes, created_at
         FROM artifacts WHERE id = ?1",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let artifact = stmt
        .query_row(params![id.as_str()], |row| {
            let producer_id_str: Option<String> = row.get(2)?;
            let producer_execution_id =
                producer_id_str.and_then(|s| std::str::FromStr::from_str(&s).ok());

            Ok(Artifact {
                id: std::str::FromStr::from_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                project_id: std::str::FromStr::from_str(&row.get::<_, String>(1)?)
                    .unwrap_or_default(),
                producer_execution_id,
                relative_path: row.get(3)?,
                content_hash: row.get(4)?,
                mime_type: row.get(5)?,
                size_bytes: row.get(6)?,
                created_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(7)?),
            })
        })
        .optional()
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    Ok(artifact)
}

pub fn list_artifacts(
    conn: &Connection,
    project_id: &str,
) -> Result<Vec<Artifact>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT
            id, project_id, producer_execution_id, relative_path,
            content_hash, mime_type, size_bytes, created_at
         FROM artifacts WHERE project_id = ?1",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let iter = stmt
        .query_map(params![project_id], |row| {
            let producer_id_str: Option<String> = row.get(2)?;
            let producer_execution_id =
                producer_id_str.and_then(|s| std::str::FromStr::from_str(&s).ok());

            Ok(Artifact {
                id: std::str::FromStr::from_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                project_id: std::str::FromStr::from_str(&row.get::<_, String>(1)?)
                    .unwrap_or_default(),
                producer_execution_id,
                relative_path: row.get(3)?,
                content_hash: row.get(4)?,
                mime_type: row.get(5)?,
                size_bytes: row.get(6)?,
                created_at: crate::datetime_util::parse_db_datetime(&row.get::<_, String>(7)?),
            })
        })
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut artifacts = Vec::new();
    for a in iter {
        artifacts.push(a.map_err(|e| Trans4mersError::Database(e.to_string()))?);
    }

    Ok(artifacts)
}
