-- Project schema M002 — locks and argument-scoped approvals

CREATE TABLE IF NOT EXISTS locks (
    id TEXT PRIMARY KEY,
    lock_key TEXT NOT NULL UNIQUE,
    holder TEXT NOT NULL,
    expires_at DATETIME NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_locks_key ON locks(lock_key, expires_at);

ALTER TABLE approvals ADD COLUMN arguments_hash TEXT;
CREATE INDEX IF NOT EXISTS idx_approvals_args ON approvals(execution_id, tool_name, arguments_hash);
