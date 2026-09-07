use arrow_array::{
    ArrayRef, FixedSizeListArray, Float32Array, RecordBatch, RecordBatchIterator,
    RecordBatchReader, StringArray,
};
use arrow_schema::{DataType, Field, Schema};
use async_trait::async_trait;
use futures::StreamExt;
use lancedb::connect;
use lancedb::connection::Connection as LanceConnection;
use lancedb::query::{ExecutableQuery, QueryBase};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use trans4mers_domain::error::Trans4mersError;

use crate::vector_store::{VectorRecord, VectorSearchResult, VectorStore};

/// Embedded LanceDB vector storage engine running in-process with zero external daemons.
/// Persists datasets in Arrow/Lance columnar format under `data/lancedb/{project_id}`.
pub struct LanceDbStore {
    db_path: PathBuf,
    connection: Mutex<Option<LanceConnection>>,
}

impl LanceDbStore {
    pub fn new(project_data_dir: impl AsRef<Path>) -> Self {
        let db_path = project_data_dir.as_ref().to_path_buf();
        Self {
            db_path,
            connection: Mutex::new(None),
        }
    }

    /// Connects lazily or returns the cached connection.
    async fn get_connection(&self) -> Result<LanceConnection, Trans4mersError> {
        let mut guard = self.connection.lock().await;
        if let Some(conn) = guard.as_ref() {
            return Ok(conn.clone());
        }

        // Ensure parent directories exist
        tokio::fs::create_dir_all(&self.db_path)
            .await
            .map_err(|e| {
                Trans4mersError::Filesystem(format!("Failed to create LanceDB dir: {}", e))
            })?;

        let path_str = self.db_path.to_string_lossy().to_string();
        let conn = connect(&path_str)
            .execute()
            .await
            .map_err(|e| Trans4mersError::Database(format!("LanceDB connection failed: {}", e)))?;

        *guard = Some(conn.clone());
        Ok(conn)
    }

    fn sanitize_table_name(table_name: &str) -> String {
        // Strip any vec_ prefix for clean Lance table naming if passed
        let clean = table_name.trim_start_matches("vec_");
        clean.to_lowercase()
    }

    fn create_record_batch(
        records: &[VectorRecord],
        dimension: u32,
    ) -> Result<RecordBatch, Trans4mersError> {
        let ids: Vec<&str> = records.iter().map(|r| r.id.as_str()).collect();
        let id_array = Arc::new(StringArray::from(ids)) as ArrayRef;

        let mut flattened_values: Vec<f32> = Vec::with_capacity(records.len() * dimension as usize);
        for r in records {
            if r.vector.len() != dimension as usize {
                return Err(Trans4mersError::Validation(format!(
                    "Vector dimension mismatch: expected {}, got {}",
                    dimension,
                    r.vector.len()
                )));
            }
            flattened_values.extend_from_slice(&r.vector);
        }

        let values_array = Arc::new(Float32Array::from(flattened_values)) as ArrayRef;
        let field = Arc::new(Field::new("item", DataType::Float32, true));
        let vector_array = Arc::new(FixedSizeListArray::new(
            field,
            dimension as i32,
            values_array,
            None,
        )) as ArrayRef;

        let metadatas: Vec<String> = records
            .iter()
            .map(|r| {
                r.metadata
                    .as_ref()
                    .map(|m| m.to_string())
                    .unwrap_or_default()
            })
            .collect();
        let metadata_array = Arc::new(StringArray::from(metadatas)) as ArrayRef;

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new(
                "vector",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    dimension as i32,
                ),
                false,
            ),
            Field::new("metadata", DataType::Utf8, true),
        ]));

        RecordBatch::try_new(schema, vec![id_array, vector_array, metadata_array])
            .map_err(|e| Trans4mersError::Internal(format!("Arrow batch creation failed: {}", e)))
    }
}

#[async_trait]
impl VectorStore for LanceDbStore {
    async fn initialize(&self, table_name: &str, dimension: u32) -> Result<(), Trans4mersError> {
        let conn = self.get_connection().await?;
        let clean_name = Self::sanitize_table_name(table_name);
        let existing_tables = conn
            .table_names()
            .execute()
            .await
            .map_err(|e| Trans4mersError::Database(e.to_string()))?;

        if !existing_tables.contains(&clean_name) {
            let empty_batch = Self::create_record_batch(&[], dimension)?;
            let schema = empty_batch.schema();
            let reader = Box::new(RecordBatchIterator::new(vec![Ok(empty_batch)], schema))
                as Box<dyn RecordBatchReader + Send>;
            conn.create_table(&clean_name, reader)
                .execute()
                .await
                .map_err(|e| {
                    Trans4mersError::Database(format!("Failed to create LanceDB table: {}", e))
                })?;
        }

        Ok(())
    }

    async fn upsert_embedding(
        &self,
        table_name: &str,
        id: &str,
        vector: &[f32],
        metadata: Option<&serde_json::Value>,
    ) -> Result<(), Trans4mersError> {
        self.batch_upsert(
            table_name,
            &[VectorRecord {
                id: id.to_string(),
                vector: vector.to_vec(),
                metadata: metadata.cloned(),
            }],
        )
        .await
    }

    async fn search_similar(
        &self,
        table_name: &str,
        query_vector: &[f32],
        top_k: usize,
        threshold: Option<f32>,
        _filters: Option<&serde_json::Value>,
    ) -> Result<Vec<VectorSearchResult>, Trans4mersError> {
        let conn = self.get_connection().await?;
        let clean_name = Self::sanitize_table_name(table_name);

        let table =
            conn.open_table(&clean_name).execute().await.map_err(|e| {
                Trans4mersError::Database(format!("LanceDB open table failed: {}", e))
            })?;

        let mut stream = table
            .vector_search(query_vector.to_vec())
            .map_err(|e| Trans4mersError::Database(e.to_string()))?
            .limit(top_k)
            .execute()
            .await
            .map_err(|e| Trans4mersError::Database(e.to_string()))?;

        let mut results = Vec::new();

        while let Some(batch_res) = stream.next().await {
            let batch = batch_res.map_err(|e| Trans4mersError::Database(e.to_string()))?;
            let id_col = batch
                .column_by_name("id")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                .ok_or_else(|| {
                    Trans4mersError::Database("Missing 'id' column in LanceDB result".to_string())
                })?;

            let dist_col = batch
                .column_by_name("_distance")
                .and_then(|c| c.as_any().downcast_ref::<Float32Array>());

            let meta_col = batch
                .column_by_name("metadata")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());

            for i in 0..batch.num_rows() {
                let id = id_col.value(i).to_string();
                let raw_dist = dist_col.map(|d| d.value(i)).unwrap_or(0.0);
                // Convert distance to normalized similarity score (0.0 to 1.0)
                let score = 1.0 / (1.0 + raw_dist.max(0.0));

                if let Some(thresh) = threshold
                    && score < thresh
                {
                    continue;
                }

                let metadata = meta_col.and_then(|m| {
                    let s = m.value(i);
                    if s.is_empty() {
                        None
                    } else {
                        serde_json::from_str(s).ok()
                    }
                });

                results.push(VectorSearchResult {
                    id,
                    score,
                    metadata,
                });
            }
        }

        Ok(results)
    }

    async fn delete_embedding(&self, table_name: &str, id: &str) -> Result<(), Trans4mersError> {
        let conn = self.get_connection().await?;
        let clean_name = Self::sanitize_table_name(table_name);
        let table = match conn.open_table(&clean_name).execute().await {
            Ok(t) => t,
            Err(_) => return Ok(()), // Table doesn't exist yet
        };

        // Escape single quotes for SQL filter
        let escaped_id = id.replace('\'', "''");
        table
            .delete(&format!("id = '{}'", escaped_id))
            .await
            .map_err(|e| Trans4mersError::Database(format!("LanceDB delete failed: {}", e)))?;

        Ok(())
    }

    async fn batch_upsert(
        &self,
        table_name: &str,
        records: &[VectorRecord],
    ) -> Result<(), Trans4mersError> {
        if records.is_empty() {
            return Ok(());
        }

        let dimension = records[0].vector.len() as u32;
        self.initialize(table_name, dimension).await?;

        let conn = self.get_connection().await?;
        let clean_name = Self::sanitize_table_name(table_name);
        let table = conn
            .open_table(&clean_name)
            .execute()
            .await
            .map_err(|e| Trans4mersError::Database(e.to_string()))?;

        // 1. Delete existing records by ID to achieve upsert behavior
        let id_list: Vec<String> = records
            .iter()
            .map(|r| format!("'{}'", r.id.replace('\'', "''")))
            .collect();
        let delete_predicate = format!("id IN ({})", id_list.join(", "));
        let _ = table.delete(&delete_predicate).await;

        // 2. Insert new records batch
        let batch = Self::create_record_batch(records, dimension)?;
        let schema = batch.schema();
        let reader = Box::new(RecordBatchIterator::new(vec![Ok(batch)], schema))
            as Box<dyn RecordBatchReader + Send>;

        table.add(reader).execute().await.map_err(|e| {
            Trans4mersError::Database(format!("LanceDB batch insert failed: {}", e))
        })?;

        Ok(())
    }

    async fn rebuild_index(&self, table_name: &str) -> Result<(), Trans4mersError> {
        let conn = self.get_connection().await?;
        let clean_name = Self::sanitize_table_name(table_name);
        let table = match conn.open_table(&clean_name).execute().await {
            Ok(t) => t,
            Err(_) => return Ok(()),
        };

        // Compact and optimize Lance datasets
        let _ = table.optimize(lancedb::table::OptimizeAction::All).await;

        Ok(())
    }

    async fn count(&self, table_name: &str) -> Result<usize, Trans4mersError> {
        let conn = self.get_connection().await?;
        let clean_name = Self::sanitize_table_name(table_name);
        let table = match conn.open_table(&clean_name).execute().await {
            Ok(t) => t,
            Err(_) => return Ok(0),
        };

        let count = table
            .count_rows(None)
            .await
            .map_err(|e| Trans4mersError::Database(e.to_string()))?;

        Ok(count)
    }
}
