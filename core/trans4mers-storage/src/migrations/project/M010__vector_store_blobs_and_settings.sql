-- Project schema M010 — Store raw vectors as BLOBs in SQLite relational tables & project_settings
-- Enables zero-cost instant index rebuilds without re-running LLM embedding inference

-- 1. Add raw embedding BLOB columns to relational tables
ALTER TABLE doc_chunks ADD COLUMN embedding BLOB;
ALTER TABLE project_memories ADD COLUMN embedding BLOB;
ALTER TABLE learned_rules ADD COLUMN embedding BLOB;

-- 2. Project settings table
CREATE TABLE IF NOT EXISTS project_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- 3. Default vector backend setting
INSERT OR IGNORE INTO project_settings (key, value) VALUES ('vector_backend', 'sqlite-vec');
