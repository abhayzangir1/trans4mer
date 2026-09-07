-- Project schema M005 — Document & Artifact RAG Pipeline, Bi-temporal Memory & Distillation Markers

-- 1. Add bi-temporal validity columns to project_memories
ALTER TABLE project_memories ADD COLUMN valid_from DATETIME;
ALTER TABLE project_memories ADD COLUMN valid_until DATETIME;

-- 2. Distillation markers tracking
CREATE TABLE IF NOT EXISTS conversation_distillation_markers (
    conversation_id TEXT PRIMARY KEY,
    last_distilled_at DATETIME NOT NULL,
    created_at DATETIME NOT NULL
);

-- 3. Document chunks & Ingestion state
CREATE TABLE IF NOT EXISTS doc_chunks (
    chunk_id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    file_path TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    ord INTEGER NOT NULL,
    line_start INTEGER NOT NULL,
    line_end INTEGER NOT NULL,
    text TEXT NOT NULL,
    kind TEXT NOT NULL,
    token_estimate INTEGER NOT NULL,
    created_at DATETIME NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_doc_chunks_file ON doc_chunks(project_id, file_path);

CREATE TABLE IF NOT EXISTS doc_ingest_state (
    project_id TEXT NOT NULL,
    file_path TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    chunk_count INTEGER NOT NULL,
    updated_at DATETIME NOT NULL,
    PRIMARY KEY(project_id, file_path)
);
