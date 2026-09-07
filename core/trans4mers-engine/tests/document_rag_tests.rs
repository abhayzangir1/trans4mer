use chrono::Utc;
use trans4mers_domain::document::{DocChunk, DocIngestState};
use trans4mers_engine::bm25::Bm25Index;
use trans4mers_engine::document_chunker::DocumentChunker;
use trans4mers_storage::db_handle::DbHandle;
use trans4mers_storage::repos::document_repo;
use uuid::Uuid;

#[test]
fn test_markdown_chunking() {
    let md_content = "\
# Overview
This document introduces the Trans4mers agent operating system.
It details the zero-trust architecture and local swarm coordination.

## Memory Architecture
The system employs a four-tier cognitive memory pyramid:
- Working memory
- Episodic memory
- Semantic memory
- Procedural memory

## Subsystem Specifications
Section 24 specifies the Document and Artifact RAG pipeline with hybrid search.
";

    let chunks =
        DocumentChunker::chunk("proj_1", "docs/architecture.md", md_content, "hash_md_123");
    assert!(
        !chunks.is_empty(),
        "Chunker must produce chunks for markdown"
    );
    assert_eq!(chunks[0].kind, "markdown");
    assert_eq!(chunks[0].line_start, 1);
    assert!(chunks[0].token_estimate > 0);
}

#[test]
fn test_code_chunking() {
    let code_content = "\
use std::sync::Arc;

pub fn initialize_engine() -> bool {
    println!(\"Starting engine\");
    true
}

pub fn execute_react_step(step: u32) -> Result<(), String> {
    if step > 50 {
        return Err(\"Max steps reached\".to_string());
    }
    Ok(())
}

pub struct EngineConfig {
    pub max_tokens: u32,
}
";

    let chunks = DocumentChunker::chunk("proj_1", "src/engine.rs", code_content, "hash_code_456");
    assert!(
        !chunks.is_empty(),
        "Chunker must produce chunks for code files"
    );
    assert_eq!(chunks[0].kind, "code");
    assert!(
        chunks
            .iter()
            .any(|c| c.text.contains("initialize_engine") || c.text.contains("execute_react_step"))
    );
}

#[test]
fn test_text_chunking() {
    let text_content = "\
First paragraph introducing the background context.
Multiple lines describing the general motivation.

Second paragraph explaining the details of the implementation.
More content elaborating on the requirements and constraints.

Third paragraph summarizing conclusions and future work.
";

    let chunks = DocumentChunker::chunk("proj_1", "notes.txt", text_content, "hash_text_789");
    assert!(!chunks.is_empty(), "Chunker must produce chunks for text");
    assert_eq!(chunks[0].kind, "text");
}

#[test]
fn test_bm25_scoring() {
    let index = Bm25Index::new();

    let docs = vec![
        (
            "doc1",
            "The SQLite database stores memory tiers and CQRS domain events",
        ),
        (
            "doc2",
            "The Chromium browser space provides an isolated browser profile",
        ),
        (
            "doc3",
            "Deterministic token compaction ensures the LLM context window does not overflow",
        ),
        (
            "doc4",
            "The database connection pool uses WAL mode and SQLite vector extension",
        ),
    ];

    let query = "database SQLite";
    let results = index.score_documents(query, &docs);

    assert!(!results.is_empty(), "BM25 should return matching documents");
    // doc1 and doc4 both contain "database" and "SQLite", so they must be top-ranked
    assert!(results[0].0 == "doc1" || results[0].0 == "doc4");
    assert!(results[1].0 == "doc1" || results[1].0 == "doc4");

    // doc2 and doc3 don't contain "database" or "SQLite", so their score should be 0 or lower
    let doc2_score = results
        .iter()
        .find(|(id, _)| id == "doc2")
        .map(|(_, s)| *s)
        .unwrap_or(0.0);
    assert_eq!(doc2_score, 0.0);
}

#[test]
fn test_document_repo_persistence() {
    let unique = format!("t4m_doc_test_{}.sqlite", Uuid::new_v4());
    let db_path = std::env::temp_dir().join(unique);
    let db = DbHandle::open(&db_path).expect("open db");

    // Run migrations
    db.with_exclusive_conn(|conn| {
        trans4mers_storage::migration_runner::MigrationRunner::run_project_migrations(conn, 768)
    })
    .expect("run migrations");

    let proj_id = "test_project_1";
    let file_path = "src/main.rs";
    let hash = "abc123hash";

    // 1. Ingest state
    let state = DocIngestState {
        project_id: proj_id.to_string(),
        file_path: file_path.to_string(),
        content_hash: hash.to_string(),
        chunk_count: 2,
        updated_at: Utc::now(),
    };

    db.with_write_tx(|conn| document_repo::upsert_ingest_state(conn, &state))
        .expect("upsert ingest state");

    let loaded_state = db
        .with_read_conn(|conn| document_repo::get_ingest_state(conn, proj_id, file_path))
        .expect("get ingest state")
        .expect("should exist");

    assert_eq!(loaded_state.content_hash, hash);
    assert_eq!(loaded_state.chunk_count, 2);

    // 2. Chunks
    let chunk1 = DocChunk {
        chunk_id: Uuid::new_v4().to_string(),
        project_id: proj_id.to_string(),
        file_path: file_path.to_string(),
        content_hash: hash.to_string(),
        ord: 0,
        line_start: 1,
        line_end: 25,
        text: "fn main() { println!(\"hello\"); }".to_string(),
        kind: "code".to_string(),
        token_estimate: 12,
        embedding: None,
        created_at: Utc::now(),
    };

    let chunk2 = DocChunk {
        chunk_id: Uuid::new_v4().to_string(),
        project_id: proj_id.to_string(),
        file_path: file_path.to_string(),
        content_hash: hash.to_string(),
        ord: 1,
        line_start: 26,
        line_end: 50,
        text: "fn helper() { true }".to_string(),
        kind: "code".to_string(),
        token_estimate: 8,
        embedding: None,
        created_at: Utc::now(),
    };

    db.with_write_tx(|conn| {
        document_repo::insert_chunk_batch(conn, &[chunk1.clone(), chunk2.clone()])
    })
    .expect("insert chunks");

    let loaded_chunks = db
        .with_read_conn(|conn| document_repo::list_chunks_by_file(conn, proj_id, file_path))
        .expect("list chunks");

    assert_eq!(loaded_chunks.len(), 2);
    assert_eq!(loaded_chunks[0].text, chunk1.text);
    assert_eq!(loaded_chunks[1].text, chunk2.text);

    // 3. Delete chunks
    db.with_write_tx(|conn| document_repo::delete_chunks_by_file(conn, proj_id, file_path))
        .expect("delete chunks");

    let after_delete = db
        .with_read_conn(|conn| document_repo::list_chunks_by_file(conn, proj_id, file_path))
        .expect("list after delete");

    assert_eq!(after_delete.len(), 0);

    let _ = std::fs::remove_file(db_path);
}
