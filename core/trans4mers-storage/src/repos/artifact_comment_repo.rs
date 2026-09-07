use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use trans4mers_domain::error::Trans4mersError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactComment {
    pub id: String,
    pub artifact_id: String,
    pub user_id: String,
    pub line_start: Option<i64>,
    pub line_end: Option<i64>,
    pub selected_text: Option<String>,
    pub comment: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

pub fn create_comment(conn: &Connection, comment: &ArtifactComment) -> Result<(), Trans4mersError> {
    conn.execute(
        "INSERT INTO artifact_comments (
            id, artifact_id, user_id, line_start, line_end, selected_text, comment, status, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            comment.id,
            comment.artifact_id,
            comment.user_id,
            comment.line_start,
            comment.line_end,
            comment.selected_text,
            comment.comment,
            comment.status,
            comment.created_at.to_rfc3339(),
        ],
    ).map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}

pub fn list_comments_for_artifact(
    conn: &Connection,
    artifact_id: &str,
) -> Result<Vec<ArtifactComment>, Trans4mersError> {
    let mut stmt = conn.prepare(
        "SELECT id, artifact_id, user_id, line_start, line_end, selected_text, comment, status, created_at
         FROM artifact_comments WHERE artifact_id = ?1 ORDER BY created_at ASC"
    ).map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let rows = stmt
        .query_map(params![artifact_id], |row| {
            let created_str: String = row.get(8)?;
            let created = crate::datetime_util::parse_db_datetime(&created_str);

            Ok(ArtifactComment {
                id: row.get(0)?,
                artifact_id: row.get(1)?,
                user_id: row.get(2)?,
                line_start: row.get(3)?,
                line_end: row.get(4)?,
                selected_text: row.get(5)?,
                comment: row.get(6)?,
                status: row.get(7)?,
                created_at: created,
            })
        })
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut result = Vec::new();
    for r in rows {
        result.push(r.map_err(|e| Trans4mersError::Database(e.to_string()))?);
    }
    Ok(result)
}

pub fn update_comment_status(
    conn: &Connection,
    id: &str,
    status: &str,
) -> Result<(), Trans4mersError> {
    conn.execute(
        "UPDATE artifact_comments SET status = ?1 WHERE id = ?2",
        params![status, id],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}

pub fn get_comment(
    conn: &Connection,
    id: &str,
) -> Result<Option<ArtifactComment>, Trans4mersError> {
    let mut stmt = conn.prepare(
        "SELECT id, artifact_id, user_id, line_start, line_end, selected_text, comment, status, created_at
         FROM artifact_comments WHERE id = ?1"
    ).map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let comment = stmt
        .query_row(params![id], |row| {
            let created_str: String = row.get(8)?;
            let created = crate::datetime_util::parse_db_datetime(&created_str);

            Ok(ArtifactComment {
                id: row.get(0)?,
                artifact_id: row.get(1)?,
                user_id: row.get(2)?,
                line_start: row.get(3)?,
                line_end: row.get(4)?,
                selected_text: row.get(5)?,
                comment: row.get(6)?,
                status: row.get(7)?,
                created_at: created,
            })
        })
        .optional()
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    Ok(comment)
}
