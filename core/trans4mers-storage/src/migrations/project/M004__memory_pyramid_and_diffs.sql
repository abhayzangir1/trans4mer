-- Project schema M004 — Memory Pyramid, ActionDiffs, Browser Spaces & Signed Events

-- 1. Alter project_memories for 4-Tier Cognitive Pyramid
ALTER TABLE project_memories ADD COLUMN tier TEXT NOT NULL DEFAULT 'Working';
ALTER TABLE project_memories ADD COLUMN retrieval_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE project_memories ADD COLUMN last_retrieved_at DATETIME;
CREATE INDEX IF NOT EXISTS idx_memories_tier ON project_memories(project_id, tier, importance);

-- 2. Create action_diffs table for Diff Review Trust Layer
CREATE TABLE IF NOT EXISTS action_diffs (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    execution_id TEXT,
    agent_instance_id TEXT,
    capability TEXT NOT NULL,
    risk_level TEXT NOT NULL,
    kind JSON NOT NULL,
    diff_payload TEXT NOT NULL,
    hunks JSON,
    decision TEXT NOT NULL DEFAULT 'Pending',
    force_review_reason TEXT,
    approval_id TEXT,
    created_at DATETIME NOT NULL,
    resolved_at DATETIME
);
CREATE INDEX IF NOT EXISTS idx_action_diffs_proj ON action_diffs(project_id, decision);

-- 3. Create browser spaces, auth vault, and snapshots
CREATE TABLE IF NOT EXISTS browser_spaces (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    name TEXT NOT NULL,
    profile_path TEXT NOT NULL,
    browser_binary TEXT,
    permissions JSON NOT NULL DEFAULT '{}',
    is_active INTEGER NOT NULL DEFAULT 0,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL
);

CREATE TABLE IF NOT EXISTS browser_auth_entries (
    id TEXT PRIMARY KEY,
    space_id TEXT NOT NULL,
    service TEXT NOT NULL,
    cookies JSON,
    local_storage JSON,
    auth_headers JSON,
    encrypted_blob BLOB,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    UNIQUE(space_id, service)
);

CREATE TABLE IF NOT EXISTS browser_snapshots (
    id TEXT PRIMARY KEY,
    space_id TEXT NOT NULL,
    reason TEXT,
    tree_hash TEXT NOT NULL,
    file_count INTEGER NOT NULL DEFAULT 0,
    total_size_bytes INTEGER NOT NULL DEFAULT 0,
    triggered_by TEXT,
    created_at DATETIME NOT NULL
);

-- 4. Cryptographic signature support for tamper-evident audit log
ALTER TABLE domain_events ADD COLUMN signature TEXT;
