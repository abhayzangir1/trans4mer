use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::ids::ProjectId;

use crate::db_handle::DbHandle;
use crate::lancedb_store::LanceDbStore;
use crate::repos::{document_repo, learned_rule_repo, memory_repo, settings_repo};
use crate::vector_store::{SqliteVecStore, VectorRecord, VectorStore};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorMigrationReport {
    pub from_backend: String,
    pub to_backend: String,
    pub doc_chunks_migrated: usize,
    pub memories_migrated: usize,
    pub rules_migrated: usize,
    pub total_migrated: usize,
    pub verification_passed: bool,
}

/// Migrates all vector embeddings for a project from one backend to another.
/// Reads all raw embeddings from SQLite relational tables (doc_chunks, project_memories, learned_rules)
/// without re-running LLM embedding inference, writes into the destination backend, verifies counts,
/// and updates project_settings.vector_backend in SQLite.
pub async fn migrate_vector_backend(
    db: &DbHandle,
    project_id: &str,
    base_lancedb_dir: &Path,
    from_backend: &str,
    to_backend: &str,
) -> Result<VectorMigrationReport, Trans4mersError> {
    if from_backend == to_backend {
        return Err(Trans4mersError::Validation(format!(
            "Source and destination vector backends are identical: {}",
            from_backend
        )));
    }

    if !["sqlite-vec", "lancedb"].contains(&from_backend)
        || !["sqlite-vec", "lancedb"].contains(&to_backend)
    {
        return Err(Trans4mersError::Validation(format!(
            "Unsupported backend: from '{}' to '{}'. Valid options: 'sqlite-vec', 'lancedb'",
            from_backend, to_backend
        )));
    }

    // 1. Read all raw embeddings from SQLite relational tables
    let (chunk_embeddings, memory_embeddings, rule_embeddings) = db.with_read_conn(|conn| {
        let chunks = document_repo::load_all_doc_chunk_embeddings(conn, project_id)?;
        let memories = memory_repo::load_all_memory_embeddings(conn, project_id)?;
        let proj_id_typed = ProjectId::from_str(project_id).ok();
        let rules = learned_rule_repo::load_all_rule_embeddings(conn, proj_id_typed.as_ref())?;
        Ok((chunks, memories, rules))
    })?;

    // 2. Prepare target store
    let target_store: Arc<dyn VectorStore> = if to_backend == "lancedb" {
        let lance_dir = base_lancedb_dir.join(project_id);
        Arc::new(LanceDbStore::new(lance_dir))
    } else {
        Arc::new(SqliteVecStore::with_db(Arc::new(db.clone())))
    };

    // 3. Batch upsert doc chunks
    let chunk_records: Vec<VectorRecord> = chunk_embeddings
        .into_iter()
        .map(|(id, vector)| VectorRecord {
            id,
            vector,
            metadata: None,
        })
        .collect();
    let doc_chunks_count = chunk_records.len();
    if !chunk_records.is_empty() {
        target_store
            .batch_upsert("doc_chunks", &chunk_records)
            .await?;
        target_store.rebuild_index("doc_chunks").await?;
    }

    // 4. Batch upsert memories
    let memory_records: Vec<VectorRecord> = memory_embeddings
        .into_iter()
        .map(|(id, vector)| VectorRecord {
            id,
            vector,
            metadata: None,
        })
        .collect();
    let memories_count = memory_records.len();
    if !memory_records.is_empty() {
        target_store
            .batch_upsert("project_memories", &memory_records)
            .await?;
        target_store.rebuild_index("project_memories").await?;
    }

    // 5. Batch upsert rules
    let rule_records: Vec<VectorRecord> = rule_embeddings
        .into_iter()
        .map(|(id, vector)| VectorRecord {
            id,
            vector,
            metadata: None,
        })
        .collect();
    let rules_count = rule_records.len();
    if !rule_records.is_empty() {
        target_store
            .batch_upsert("learned_rules", &rule_records)
            .await?;
        target_store.rebuild_index("learned_rules").await?;
    }

    // 6. Verify counts match
    let target_chunks_count = target_store.count("doc_chunks").await?;
    let target_memories_count = target_store.count("project_memories").await?;
    let target_rules_count = target_store.count("learned_rules").await?;

    let verification_passed = target_chunks_count == doc_chunks_count
        && target_memories_count == memories_count
        && target_rules_count == rules_count;

    if !verification_passed {
        return Err(Trans4mersError::Validation(format!(
            "Vector migration count mismatch: doc_chunks ({}/{}), memories ({}/{}), rules ({}/{})",
            doc_chunks_count,
            target_chunks_count,
            memories_count,
            target_memories_count,
            rules_count,
            target_rules_count
        )));
    }

    // 7. Update project_settings.vector_backend in SQLite write transaction
    db.with_write_tx(|tx| settings_repo::set_vector_backend(tx, to_backend))?;

    Ok(VectorMigrationReport {
        from_backend: from_backend.to_string(),
        to_backend: to_backend.to_string(),
        doc_chunks_migrated: doc_chunks_count,
        memories_migrated: memories_count,
        rules_migrated: rules_count,
        total_migrated: doc_chunks_count + memories_count + rules_count,
        verification_passed,
    })
}
