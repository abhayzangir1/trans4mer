-- Global schema M003 — Cost Ledger, Cost Budgets & MCP Server Registry

CREATE TABLE IF NOT EXISTS cost_entries (
    id TEXT PRIMARY KEY,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    project_id TEXT,
    execution_id TEXT,
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    cost_usd REAL NOT NULL DEFAULT 0.0,
    is_fallback INTEGER NOT NULL DEFAULT 0,
    created_at DATETIME NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_cost_entries_provider ON cost_entries(provider, created_at);

CREATE TABLE IF NOT EXISTS cost_budgets (
    id TEXT PRIMARY KEY,
    project_id TEXT,
    provider TEXT NOT NULL,
    monthly_ceiling_usd REAL,
    daily_ceiling_usd REAL,
    hard_block INTEGER NOT NULL DEFAULT 1,
    alert_thresholds JSON NOT NULL DEFAULT '[0.5,0.8,1.0]',
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS mcp_server_registry (
    name TEXT PRIMARY KEY,
    command TEXT NOT NULL,
    args JSON NOT NULL DEFAULT '[]',
    env JSON NOT NULL DEFAULT '{}',
    auto_launch INTEGER NOT NULL DEFAULT 0,
    approved INTEGER NOT NULL DEFAULT 0,
    notes TEXT,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL
);
