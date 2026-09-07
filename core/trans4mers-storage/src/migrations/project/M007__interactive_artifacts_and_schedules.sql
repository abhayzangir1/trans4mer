-- Project schema M007 — Interactive Artifact Comments & Continuous Scheduled Automations

CREATE TABLE IF NOT EXISTS artifact_comments (
    id TEXT PRIMARY KEY,
    artifact_id TEXT NOT NULL,
    user_id TEXT NOT NULL DEFAULT 'human',
    line_start INTEGER,
    line_end INTEGER,
    selected_text TEXT,
    comment TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'open',
    created_at DATETIME NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_artifact_comments_art ON artifact_comments(artifact_id);

CREATE TABLE IF NOT EXISTS scheduled_tasks (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    conversation_id TEXT NOT NULL,
    target_agent_id TEXT NOT NULL,
    cron_expression TEXT NOT NULL,
    human_readable TEXT NOT NULL,
    action_prompt TEXT NOT NULL,
    is_active INTEGER NOT NULL DEFAULT 1,
    last_run_at DATETIME,
    next_run_at DATETIME,
    created_at DATETIME NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_scheduled_tasks_active ON scheduled_tasks(is_active, next_run_at);
