use rusqlite::{Connection, OptionalExtension, params};
use trans4mers_domain::document::{DocChunk, DocIngestState};
use trans4mers_domain::error::Trans4mersError;

pub fn upsert_ingest_state(
    conn: &Connection,
    state: &DocIngestState,
) -> Result<(), Trans4mersError> {
    conn.execute(
        "INSERT INTO doc_ingest_state (project_id, file_path, content_hash, chunk_count, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(project_id, file_path) DO UPDATE SET
            content_hash = excluded.content_hash,
            chunk_count = excluded.chunk_count,
            updated_at = excluded.updated_at",
        params![
            state.project_id,
            state.file_path,
            state.content_hash,
            state.chunk_count,
            state.updated_at.to_rfc3339(),
        ],
    ).map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}

pub fn get_ingest_state(
    conn: &Connection,
    project_id: &str,
    file_path: &str,
) -> Result<Option<DocIngestState>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT project_id, file_path, content_hash, chunk_count, updated_at
         FROM doc_ingest_state
         WHERE project_id = ?1 AND file_path = ?2",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let res = stmt
        .query_row(params![project_id, file_path], |row| {
            let updated_str: String = row.get(4)?;
            Ok(DocIngestState {
                project_id: row.get(0)?,
                file_path: row.get(1)?,
                content_hash: row.get(2)?,
                chunk_count: row.get(3)?,
                updated_at: crate::datetime_util::parse_db_datetime(&updated_str),
            })
        })
        .optional()
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    Ok(res)
}

pub fn list_ingest_states(
    conn: &Connection,
    project_id: &str,
) -> Result<Vec<DocIngestState>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT project_id, file_path, content_hash, chunk_count, updated_at
         FROM doc_ingest_state
         WHERE project_id = ?1
         ORDER BY file_path ASC",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let rows = stmt
        .query_map(params![project_id], |row| {
            let updated_str: String = row.get(4)?;
            Ok(DocIngestState {
                project_id: row.get(0)?,
                file_path: row.get(1)?,
                content_hash: row.get(2)?,
                chunk_count: row.get(3)?,
                updated_at: crate::datetime_util::parse_db_datetime(&updated_str),
            })
        })
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r.map_err(|e| Trans4mersError::Database(e.to_string()))?);
    }
    Ok(list)
}

pub fn delete_chunks_by_file(
    conn: &Connection,
    project_id: &str,
    file_path: &str,
) -> Result<(), Trans4mersError> {
    // 1. Clean up ghost vectors from vec_doc_chunks and vec_id_map first
    if let Ok(mut get_ids_stmt) =
        conn.prepare("SELECT chunk_id FROM doc_chunks WHERE project_id = ?1 AND file_path = ?2")
    {
        let chunk_ids_res = get_ids_stmt.query_map(params![project_id, file_path], |row| {
            row.get::<_, String>(0)
        });
        if let Ok(chunk_ids) = chunk_ids_res {
            for cid in chunk_ids.filter_map(Result::ok) {
                if let Ok(rowid) = conn.query_row(
                    "SELECT rowid FROM vec_id_map WHERE uuid = ?1",
                    [&cid],
                    |r| r.get::<_, i64>(0),
                ) {
                    let _ = conn.execute("DELETE FROM vec_doc_chunks WHERE rowid = ?1", [rowid]);
                    let _ = conn.execute("DELETE FROM vec_id_map WHERE rowid = ?1", [rowid]);
                }
            }
        }
    }

    // 2. Delete chunk records
    conn.execute(
        "DELETE FROM doc_chunks WHERE project_id = ?1 AND file_path = ?2",
        params![project_id, file_path],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}

pub fn insert_chunk_batch(conn: &Connection, chunks: &[DocChunk]) -> Result<(), Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "INSERT INTO doc_chunks (
            chunk_id, project_id, file_path, content_hash, ord,
            line_start, line_end, text, kind, token_estimate, created_at, embedding
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    for c in chunks {
        let embedding_bytes = c
            .embedding
            .as_ref()
            .map(|e| crate::vector_blob::embedding_to_blob(e));
        stmt.execute(params![
            c.chunk_id,
            c.project_id,
            c.file_path,
            c.content_hash,
            c.ord,
            c.line_start,
            c.line_end,
            c.text,
            c.kind,
            c.token_estimate,
            c.created_at.to_rfc3339(),
            embedding_bytes,
        ])
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    }

    Ok(())
}

/// Loads all document chunk IDs and their raw vector embeddings from SQLite relational table.
/// Used for zero-inference instant vector index rebuilds and migrations.
pub fn load_all_doc_chunk_embeddings(
    conn: &Connection,
    project_id: &str,
) -> Result<Vec<(String, Vec<f32>)>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT chunk_id, embedding
             FROM doc_chunks
             WHERE project_id = ?1 AND embedding IS NOT NULL",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let rows = stmt
        .query_map(params![project_id], |row| {
            let id: String = row.get(0)?;
            let blob: Vec<u8> = row.get(1)?;
            let emb = crate::vector_blob::blob_to_embedding(&blob);
            Ok((id, emb))
        })
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut results = Vec::new();
    for r in rows {
        results.push(r.map_err(|e| Trans4mersError::Database(e.to_string()))?);
    }
    Ok(results)
}

pub fn list_chunks_by_file(
    conn: &Connection,
    project_id: &str,
    file_path: &str,
) -> Result<Vec<DocChunk>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT chunk_id, project_id, file_path, content_hash, ord,
                line_start, line_end, text, kind, token_estimate, created_at, embedding
         FROM doc_chunks
         WHERE project_id = ?1 AND file_path = ?2
         ORDER BY ord ASC",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let rows = stmt
        .query_map(params![project_id, file_path], row_to_chunk)
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r.map_err(|e| Trans4mersError::Database(e.to_string()))?);
    }
    Ok(list)
}

pub fn list_all_chunks_for_project(
    conn: &Connection,
    project_id: &str,
) -> Result<Vec<DocChunk>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT chunk_id, project_id, file_path, content_hash, ord,
                line_start, line_end, text, kind, token_estimate, created_at, embedding
         FROM doc_chunks
         WHERE project_id = ?1
         ORDER BY file_path ASC, ord ASC",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let rows = stmt
        .query_map(params![project_id], row_to_chunk)
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r.map_err(|e| Trans4mersError::Database(e.to_string()))?);
    }
    Ok(list)
}

pub fn get_chunk_by_id(
    conn: &Connection,
    chunk_id: &str,
) -> Result<Option<DocChunk>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT chunk_id, project_id, file_path, content_hash, ord,
                line_start, line_end, text, kind, token_estimate, created_at, embedding
         FROM doc_chunks
         WHERE chunk_id = ?1",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let res = stmt
        .query_row(params![chunk_id], row_to_chunk)
        .optional()
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    Ok(res)
}

fn row_to_chunk(row: &rusqlite::Row) -> rusqlite::Result<DocChunk> {
    let created_str: String = row.get(10)?;
    let embedding = row
        .get::<_, Option<Vec<u8>>>(11)
        .ok()
        .flatten()
        .map(|blob| crate::vector_blob::blob_to_embedding(&blob));

    Ok(DocChunk {
        chunk_id: row.get(0)?,
        project_id: row.get(1)?,
        file_path: row.get(2)?,
        content_hash: row.get(3)?,
        ord: row.get(4)?,
        line_start: row.get(5)?,
        line_end: row.get(6)?,
        text: row.get(7)?,
        kind: row.get(8)?,
        token_estimate: row.get(9)?,
        created_at: crate::datetime_util::parse_db_datetime(&created_str),
        embedding,
    })
}

/// Searches document chunks using the SQLite FTS5 external content table and BM25 ranking.
/// Returns matching chunks and their BM25 score (where lower/more negative is better match).
pub fn search_chunks_fts5(
    conn: &Connection,
    project_id: &str,
    sanitized_match_query: &str,
    file_pattern: Option<&str>,
    limit: usize,
) -> Result<Vec<(DocChunk, f32)>, Trans4mersError> {
    if sanitized_match_query.trim().is_empty() {
        return Ok(Vec::new());
    }

    let mut sql = String::from(
        "SELECT c.chunk_id, c.project_id, c.file_path, c.content_hash, c.ord,
                c.line_start, c.line_end, c.text, c.kind, c.token_estimate, c.created_at,
                c.embedding,
                bm25(doc_chunks_fts) AS bm25_score
         FROM doc_chunks c
         JOIN doc_chunks_fts ON doc_chunks_fts.rowid = c.rowid
         WHERE doc_chunks_fts MATCH ?1
           AND c.project_id = ?2",
    );

    let mut params_vec: Vec<rusqlite::types::Value> = vec![
        rusqlite::types::Value::Text(sanitized_match_query.to_string()),
        rusqlite::types::Value::Text(project_id.to_string()),
    ];

    if let Some(pattern) = file_pattern {
        sql.push_str(" AND c.file_path LIKE ?3");
        params_vec.push(rusqlite::types::Value::Text(format!("%{}%", pattern)));
    }

    let limit_idx = params_vec.len() + 1;
    sql.push_str(&format!(" ORDER BY bm25_score ASC LIMIT ?{}", limit_idx));
    params_vec.push(rusqlite::types::Value::Integer(limit as i64));

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let rows = stmt
        .query_map(rusqlite::params_from_iter(params_vec), |row| {
            let chunk = row_to_chunk(row)?;
            let bm25_score: f64 = row.get(12)?;
            Ok((chunk, bm25_score as f32))
        })
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut results = Vec::new();
    for r in rows {
        results.push(r.map_err(|e| Trans4mersError::Database(e.to_string()))?);
    }

    Ok(results)
}
