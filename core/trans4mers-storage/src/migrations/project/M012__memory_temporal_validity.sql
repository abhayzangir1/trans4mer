-- Migration 012: Temporal validity, replacement lineage, and content hashing for cognitive memories
ALTER TABLE project_memories ADD COLUMN valid_at DATETIME;
ALTER TABLE project_memories ADD COLUMN invalid_at DATETIME;
ALTER TABLE project_memories ADD COLUMN replaced_by TEXT;
ALTER TABLE project_memories ADD COLUMN content_hash TEXT;

-- Backfill valid_at from existing valid_from or created_at
UPDATE project_memories SET valid_at = COALESCE(valid_from, created_at) WHERE valid_at IS NULL;
-- Backfill invalid_at from valid_until if valid_until is set
UPDATE project_memories SET invalid_at = valid_until WHERE invalid_at IS NULL AND valid_until IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_memories_temporal ON project_memories(project_id, valid_at, invalid_at);
CREATE INDEX IF NOT EXISTS idx_memories_content_hash ON project_memories(project_id, content_hash);
