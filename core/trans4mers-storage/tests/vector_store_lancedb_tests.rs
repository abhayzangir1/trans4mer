use chrono::Utc;
use std::sync::Arc;
use trans4mers_domain::document::DocChunk;
use trans4mers_domain::ids::{AgentInstanceId, ConversationId, LearnedRuleId, MemoryId, ProjectId};
use trans4mers_domain::learning::{Confidence, LearnedRule, RuleMetadata};
use trans4mers_domain::memory::{
    Memory, MemoryLifecycle, MemoryProvenance, MemoryScope, MemoryTier,
};
use trans4mers_storage::db_handle::DbHandle;
use trans4mers_storage::lancedb_store::LanceDbStore;
use trans4mers_storage::migration_runner::MigrationRunner;
use trans4mers_storage::repos::{document_repo, learned_rule_repo, memory_repo, settings_repo};
use trans4mers_storage::vector_migration::migrate_vector_backend;
use trans4mers_storage::vector_store::{SqliteVecStore, VectorRecord, VectorStore};
use uuid::Uuid;

#[tokio::test]
async fn test_lancedb_crud_and_similarity() {
    let tmp_dir = std::env::temp_dir().join(format!("test_lance_crud_{}", Uuid::new_v4()));
    let store = LanceDbStore::new(&tmp_dir);

    // Initial count of empty table
    assert_eq!(store.count("doc_chunks").await.unwrap(), 0);

    // Prepare test records with 4-dimensional embeddings
    let records = vec![
        VectorRecord {
            id: "apple".to_string(),
            vector: vec![1.0, 0.0, 0.0, 0.0],
            metadata: None,
        },
        VectorRecord {
            id: "banana".to_string(),
            vector: vec![0.0, 1.0, 0.0, 0.0],
            metadata: None,
        },
        VectorRecord {
            id: "cherry".to_string(),
            vector: vec![0.9, 0.1, 0.0, 0.0],
            metadata: None,
        },
    ];

    // Batch upsert into LanceDB
    store.batch_upsert("doc_chunks", &records).await.unwrap();
    assert_eq!(store.count("doc_chunks").await.unwrap(), 3);

    // Search similar to apple: top match must be apple, second cherry
    let matches = store
        .search_similar("doc_chunks", &[1.0, 0.0, 0.0, 0.0], 2, None, None)
        .await
        .unwrap();
    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0].id, "apple");
    assert_eq!(matches[1].id, "cherry");

    // Upsert a single embedding (modifying banana)
    store
        .upsert_embedding("doc_chunks", "banana", &[0.0, 0.9, 0.1, 0.0], None)
        .await
        .unwrap();
    assert_eq!(store.count("doc_chunks").await.unwrap(), 3);

    // Delete cherry
    store
        .delete_embedding("doc_chunks", "cherry")
        .await
        .unwrap();
    assert_eq!(store.count("doc_chunks").await.unwrap(), 2);

    let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
}

#[tokio::test]
async fn test_sqlite_vec_async_trait() {
    let unique = format!("t4m_sqlite_vec_trait_{}.sqlite", Uuid::new_v4());
    let db_path = std::env::temp_dir().join(unique);
    let db = DbHandle::open(&db_path).expect("open db");

    db.with_exclusive_conn(|conn| MigrationRunner::run_project_migrations(conn, 4))
        .expect("run project migrations");

    let store: Arc<dyn VectorStore> = Arc::new(SqliteVecStore::with_db(Arc::new(db)));

    // Batch upsert
    let records = vec![
        VectorRecord {
            id: "doc_1".to_string(),
            vector: vec![1.0, 0.0, 0.0, 0.0],
            metadata: None,
        },
        VectorRecord {
            id: "doc_2".to_string(),
            vector: vec![0.0, 1.0, 0.0, 0.0],
            metadata: None,
        },
    ];

    store.batch_upsert("doc_chunks", &records).await.unwrap();
    assert_eq!(store.count("doc_chunks").await.unwrap(), 2);

    // Search
    let results = store
        .search_similar("doc_chunks", &[1.0, 0.0, 0.0, 0.0], 1, None, None)
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "doc_1");

    // Delete
    store.delete_embedding("doc_chunks", "doc_1").await.unwrap();
    assert_eq!(store.count("doc_chunks").await.unwrap(), 1);

    let _ = std::fs::remove_file(db_path);
}

#[tokio::test]
async fn test_vector_migration_sqlite_to_lancedb_and_back() {
    let unique = format!("t4m_vec_mig_{}.sqlite", Uuid::new_v4());
    let db_path = std::env::temp_dir().join(&unique);
    let db = DbHandle::open(&db_path).expect("open db");

    db.with_exclusive_conn(|conn| MigrationRunner::run_project_migrations(conn, 4))
        .expect("run project migrations");

    let proj_id_typed = ProjectId::new();
    let proj_id_str = proj_id_typed.as_str();
    let proj_id = proj_id_str.as_str();

    // 1. Seed relational tables with BLOB embeddings
    db.with_write_tx(|conn| {
        // Insert doc chunk with embedding
        let chunk = DocChunk {
            chunk_id: "chunk_001".to_string(),
            project_id: proj_id.to_string(),
            file_path: "src/main.rs".to_string(),
            content_hash: "hash_001".to_string(),
            ord: 0,
            line_start: 1,
            line_end: 10,
            text: "fn main() {}".to_string(),
            kind: "code".to_string(),
            token_estimate: 4,
            embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
            created_at: Utc::now(),
        };
        document_repo::insert_chunk_batch(conn, &[chunk])?;

        // Insert project memory with embedding
        let mem = Memory {
            id: MemoryId::new(),
            project_id: proj_id_typed,
            conversation_id: Some(ConversationId::new()),
            agent_instance_id: Some(AgentInstanceId::new()),
            scope: MemoryScope::Project,
            lifecycle: MemoryLifecycle::Persisted,
            content: "User prefers async LanceDB for large vector scale".to_string(),
            importance: 0.9,
            confidence: 0.95,
            provenance: MemoryProvenance {
                source_event_id: None,
                source_message_id: None,
                source_agent_id: None,
                creation_reason: "Explicit user instruction".to_string(),
            },
            depth_level: 0,
            tier: MemoryTier::Working,
            retrieval_count: 0,
            last_retrieved_at: None,
            expires_at: None,
            visibility_overrides: None,
            valid_from: None,
            valid_until: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            embedding: Some(vec![0.0, 1.0, 0.0, 0.0]),
            valid_at: Some(Utc::now()),
            invalid_at: None,
            replaced_by: None,
            content_hash: None,
        };
        memory_repo::insert_project_memory(conn, &mem)?;

        // Insert learned rule with embedding
        let rule = LearnedRule {
            id: LearnedRuleId::new(),
            project_id: Some(proj_id_typed),
            rule_text: "Always check zero network egress before executing tools".to_string(),
            confidence: Confidence::High,
            metadata: RuleMetadata {
                language: None,
                framework: None,
                file_pattern: None,
                source_agent_id: None,
                source_execution_id: None,
            },
            embedding: Some(vec![0.0, 0.0, 1.0, 0.0]),
            created_at: Utc::now(),
        };
        learned_rule_repo::insert_rule(conn, &rule)?;

        // Seed sqlite-vec virtual table for chunk
        let vec_store = SqliteVecStore::new();
        vec_store.upsert_embedding(conn, "vec_doc_chunks", "chunk_001", &[1.0, 0.0, 0.0, 0.0])?;

        Ok(())
    })
    .expect("seed data");

    // Verify initial backend is sqlite-vec
    let init_backend = db
        .with_read_conn(|conn| settings_repo::get_vector_backend(conn))
        .unwrap();
    assert_eq!(init_backend, "sqlite-vec");

    let lance_base_dir = std::env::temp_dir().join(format!("test_lance_base_{}", Uuid::new_v4()));

    // 2. Migrate from sqlite-vec to LanceDB
    let report = migrate_vector_backend(&db, proj_id, &lance_base_dir, "sqlite-vec", "lancedb")
        .await
        .expect("migration to lancedb");

    assert_eq!(report.from_backend, "sqlite-vec");
    assert_eq!(report.to_backend, "lancedb");
    assert_eq!(report.doc_chunks_migrated, 1);
    assert_eq!(report.memories_migrated, 1);
    assert_eq!(report.rules_migrated, 1);
    assert_eq!(report.total_migrated, 3);
    assert!(report.verification_passed);

    // Verify project setting is updated to lancedb
    let current_backend = db
        .with_read_conn(|conn| settings_repo::get_vector_backend(conn))
        .unwrap();
    assert_eq!(current_backend, "lancedb");

    // Verify LanceDbStore can search the migrated data
    let lance_dir = lance_base_dir.join(proj_id);
    let lance_store = LanceDbStore::new(&lance_dir);
    let search_res = lance_store
        .search_similar("doc_chunks", &[1.0, 0.0, 0.0, 0.0], 1, None, None)
        .await
        .unwrap();
    assert_eq!(search_res.len(), 1);
    assert_eq!(search_res[0].id, "chunk_001");

    // 3. Migrate back from LanceDB to sqlite-vec
    let reverse_report =
        migrate_vector_backend(&db, proj_id, &lance_base_dir, "lancedb", "sqlite-vec")
            .await
            .expect("reverse migration to sqlite-vec");

    assert_eq!(reverse_report.from_backend, "lancedb");
    assert_eq!(reverse_report.to_backend, "sqlite-vec");
    assert_eq!(reverse_report.total_migrated, 3);
    assert!(reverse_report.verification_passed);

    // Verify setting is reverted to sqlite-vec
    let reverted_backend = db
        .with_read_conn(|conn| settings_repo::get_vector_backend(conn))
        .unwrap();
    assert_eq!(reverted_backend, "sqlite-vec");

    // 4. Verification of invalid migrations
    let err_same =
        migrate_vector_backend(&db, proj_id, &lance_base_dir, "sqlite-vec", "sqlite-vec").await;
    assert!(err_same.is_err());

    let err_invalid =
        migrate_vector_backend(&db, proj_id, &lance_base_dir, "sqlite-vec", "pinecone").await;
    assert!(err_invalid.is_err());

    let _ = std::fs::remove_file(db_path);
    let _ = tokio::fs::remove_dir_all(&lance_base_dir).await;
}
