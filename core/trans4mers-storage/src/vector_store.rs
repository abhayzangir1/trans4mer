use async_trait::async_trait;
use rusqlite::{Connection, Row, params};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use trans4mers_domain::error::Trans4mersError;

use crate::db_handle::DbHandle;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorSearchResult {
    pub id: String,
    pub score: f32,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorRecord {
    pub id: String,
    pub vector: Vec<f32>,
    pub metadata: Option<serde_json::Value>,
}

#[async_trait]
pub trait VectorStore: Send + Sync {
    /// Initialize or ensure table/dataset exists for a given category and vector dimension.
    async fn initialize(&self, table_name: &str, dimension: u32) -> Result<(), Trans4mersError>;

    /// Insert or update an embedding for a specific record.
    async fn upsert_embedding(
        &self,
        table_name: &str,
        id: &str,
        vector: &[f32],
        metadata: Option<&serde_json::Value>,
    ) -> Result<(), Trans4mersError>;

    /// Search for semantically similar records.
    async fn search_similar(
        &self,
        table_name: &str,
        query_vector: &[f32],
        top_k: usize,
        threshold: Option<f32>,
        filters: Option<&serde_json::Value>,
    ) -> Result<Vec<VectorSearchResult>, Trans4mersError>;

    /// Delete an embedding for a specific record.
    async fn delete_embedding(&self, table_name: &str, id: &str) -> Result<(), Trans4mersError>;

    /// Batch upsert embeddings.
    async fn batch_upsert(
        &self,
        table_name: &str,
        records: &[VectorRecord],
    ) -> Result<(), Trans4mersError>;

    /// Rebuild or optimize index.
    async fn rebuild_index(&self, table_name: &str) -> Result<(), Trans4mersError>;

    /// Count embeddings stored in the index.
    async fn count(&self, table_name: &str) -> Result<usize, Trans4mersError>;
}

/// Zero-dependency SQLite virtual table backend (sqlite-vec / vec0).
#[derive(Default, Clone)]
pub struct SqliteVecStore {
    db: Option<Arc<DbHandle>>,
}

impl SqliteVecStore {
    pub fn new() -> Self {
        Self { db: None }
    }

    pub fn with_db(db: Arc<DbHandle>) -> Self {
        Self { db: Some(db) }
    }

    pub fn validate_table_name(table_name: &str) -> Result<&'static str, Trans4mersError> {
        match table_name {
            "vec_project_memories" | "project_memories" => Ok("vec_project_memories"),
            "vec_learned_rules" | "learned_rules" => Ok("vec_learned_rules"),
            "vec_doc_chunks" | "doc_chunks" => Ok("vec_doc_chunks"),
            _ => Err(Trans4mersError::Database(format!(
                "Invalid vector table name: {}",
                table_name
            ))),
        }
    }

    pub fn ensure_mapping(conn: &Connection, uuid: &str) -> Result<i64, Trans4mersError> {
        conn.execute(
            "INSERT OR IGNORE INTO vec_id_map (uuid) VALUES (?1)",
            params![uuid],
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

        let rowid: i64 = conn
            .query_row(
                "SELECT rowid FROM vec_id_map WHERE uuid = ?1",
                params![uuid],
                |row| row.get(0),
            )
            .map_err(|e| Trans4mersError::Database(e.to_string()))?;

        Ok(rowid)
    }

    // Direct synchronous connection methods for callers operating within transactions:
    pub fn upsert_embedding_conn(
        &self,
        conn: &Connection,
        table_name: &str,
        record_uuid: &str,
        embedding: &[f32],
    ) -> Result<(), Trans4mersError> {
        let valid_table = Self::validate_table_name(table_name)?;
        let rowid = Self::ensure_mapping(conn, record_uuid)?;

        let embedding_bytes = crate::vector_blob::embedding_to_blob(embedding);

        // SQLite virtual tables (vec0) do not support ON CONFLICT(rowid) DO UPDATE.
        let delete_query = format!("DELETE FROM {} WHERE rowid = ?1;", valid_table);
        let _ = conn.execute(&delete_query, params![rowid]);

        let insert_query = format!(
            "INSERT INTO {} (rowid, embedding) VALUES (?1, ?2);",
            valid_table
        );
        conn.execute(&insert_query, params![rowid, embedding_bytes])
            .map_err(|e| Trans4mersError::Database(format!("Vector insertion failed: {}", e)))?;

        Ok(())
    }

    pub fn delete_embedding_conn(
        &self,
        conn: &Connection,
        table_name: &str,
        record_uuid: &str,
    ) -> Result<(), Trans4mersError> {
        let valid_table = Self::validate_table_name(table_name)?;
        let rowid_opt: Option<i64> = conn
            .query_row(
                "SELECT rowid FROM vec_id_map WHERE uuid = ?1",
                params![record_uuid],
                |row| row.get(0),
            )
            .ok();

        if let Some(rowid) = rowid_opt {
            let delete_vec = format!("DELETE FROM {} WHERE rowid = ?1;", valid_table);
            let _ = conn.execute(&delete_vec, params![rowid]);
            let _ = conn.execute("DELETE FROM vec_id_map WHERE rowid = ?1;", params![rowid]);
        }

        Ok(())
    }

    pub fn search_similar_conn(
        &self,
        conn: &Connection,
        table_name: &str,
        query_embedding: &[f32],
        limit: u32,
    ) -> Result<Vec<String>, Trans4mersError> {
        let valid_table = Self::validate_table_name(table_name)?;
        let embedding_bytes = crate::vector_blob::embedding_to_blob(query_embedding);

        let query = format!(
            "SELECT m.uuid
             FROM {} v
             JOIN vec_id_map m ON v.rowid = m.rowid
             WHERE v.embedding MATCH ?1 AND k = ?2",
            valid_table
        );

        let mut stmt = conn
            .prepare(&query)
            .map_err(|e| Trans4mersError::Database(e.to_string()))?;

        let rows = stmt
            .query_map(params![embedding_bytes, limit], |row: &Row| {
                let id_str: String = row.get(0)?;
                Ok(id_str)
            })
            .map_err(|e| Trans4mersError::Database(e.to_string()))?;

        let mut results = Vec::new();
        for result in rows {
            let id_str = result.map_err(|e| Trans4mersError::Database(e.to_string()))?;
            results.push(id_str);
        }

        Ok(results)
    }

    // Synchronous aliases matching earlier VectorStore method signatures:
    pub fn upsert_embedding(
        &self,
        conn: &Connection,
        table_name: &str,
        record_uuid: &str,
        embedding: &[f32],
    ) -> Result<(), Trans4mersError> {
        self.upsert_embedding_conn(conn, table_name, record_uuid, embedding)
    }

    pub fn delete_embedding(
        &self,
        conn: &Connection,
        table_name: &str,
        record_uuid: &str,
    ) -> Result<(), Trans4mersError> {
        self.delete_embedding_conn(conn, table_name, record_uuid)
    }

    pub fn search_similar(
        &self,
        conn: &Connection,
        table_name: &str,
        query_embedding: &[f32],
        limit: u32,
    ) -> Result<Vec<String>, Trans4mersError> {
        self.search_similar_conn(conn, table_name, query_embedding, limit)
    }
}

#[async_trait]
impl VectorStore for SqliteVecStore {
    async fn initialize(&self, _table_name: &str, _dimension: u32) -> Result<(), Trans4mersError> {
        // Tables are initialized during migrations.
        Ok(())
    }

    async fn upsert_embedding(
        &self,
        table_name: &str,
        id: &str,
        vector: &[f32],
        _metadata: Option<&serde_json::Value>,
    ) -> Result<(), Trans4mersError> {
        let db = self.db.as_ref().ok_or_else(|| {
            Trans4mersError::Internal("SqliteVecStore has no DbHandle configured".to_string())
        })?;

        let table = table_name.to_string();
        let record_id = id.to_string();
        let vec_data = vector.to_vec();

        db.with_write_tx(|tx| self.upsert_embedding_conn(tx, &table, &record_id, &vec_data))
    }

    async fn search_similar(
        &self,
        table_name: &str,
        query_vector: &[f32],
        top_k: usize,
        threshold: Option<f32>,
        _filters: Option<&serde_json::Value>,
    ) -> Result<Vec<VectorSearchResult>, Trans4mersError> {
        let db = self.db.as_ref().ok_or_else(|| {
            Trans4mersError::Internal("SqliteVecStore has no DbHandle configured".to_string())
        })?;

        let valid_table = Self::validate_table_name(table_name)?;
        let embedding_bytes = crate::vector_blob::embedding_to_blob(query_vector);
        let limit = top_k as u32;

        let ids_with_dist = db.with_read_conn(|conn| {
            // In sqlite-vec, distance is available via distance column
            let query = format!(
                "SELECT m.uuid, distance
                 FROM {} v
                 JOIN vec_id_map m ON v.rowid = m.rowid
                 WHERE v.embedding MATCH ?1 AND k = ?2",
                valid_table
            );

            let mut stmt = conn
                .prepare(&query)
                .map_err(|e| Trans4mersError::Database(e.to_string()))?;

            let rows = stmt
                .query_map(params![embedding_bytes, limit], |row: &Row| {
                    let id_str: String = row.get(0)?;
                    let dist: f64 = row.get(1).unwrap_or(0.0);
                    Ok((id_str, dist as f32))
                })
                .map_err(|e| Trans4mersError::Database(e.to_string()))?;

            let mut out = Vec::new();
            for r in rows {
                out.push(r.map_err(|e| Trans4mersError::Database(e.to_string()))?);
            }
            Ok(out)
        })?;

        let mut results = Vec::new();
        for (id, dist) in ids_with_dist {
            let score = 1.0 / (1.0 + dist.max(0.0));
            if let Some(t) = threshold
                && score < t
            {
                continue;
            }
            results.push(VectorSearchResult {
                id,
                score,
                metadata: None,
            });
        }

        Ok(results)
    }

    async fn delete_embedding(&self, table_name: &str, id: &str) -> Result<(), Trans4mersError> {
        let db = self.db.as_ref().ok_or_else(|| {
            Trans4mersError::Internal("SqliteVecStore has no DbHandle configured".to_string())
        })?;

        let table = table_name.to_string();
        let record_id = id.to_string();

        db.with_write_tx(|tx| self.delete_embedding_conn(tx, &table, &record_id))
    }

    async fn batch_upsert(
        &self,
        table_name: &str,
        records: &[VectorRecord],
    ) -> Result<(), Trans4mersError> {
        let db = self.db.as_ref().ok_or_else(|| {
            Trans4mersError::Internal("SqliteVecStore has no DbHandle configured".to_string())
        })?;

        let valid_table = Self::validate_table_name(table_name)?;
        db.with_write_tx(|tx| {
            for r in records {
                self.upsert_embedding_conn(tx, valid_table, &r.id, &r.vector)?;
            }
            Ok(())
        })
    }

    async fn rebuild_index(&self, _table_name: &str) -> Result<(), Trans4mersError> {
        // sqlite-vec maintains its index on write; no explicit rebuild required.
        Ok(())
    }

    async fn count(&self, table_name: &str) -> Result<usize, Trans4mersError> {
        let db = self.db.as_ref().ok_or_else(|| {
            Trans4mersError::Internal("SqliteVecStore has no DbHandle configured".to_string())
        })?;

        let valid_table = Self::validate_table_name(table_name)?;
        db.with_read_conn(|conn| {
            let query = format!("SELECT COUNT(*) FROM {}", valid_table);
            let count: i64 = conn.query_row(&query, [], |row| row.get(0)).unwrap_or(0);
            Ok(count as usize)
        })
    }
}
