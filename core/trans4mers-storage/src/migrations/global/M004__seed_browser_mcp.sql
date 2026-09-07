-- Global schema M004 — Seed pre-vetted, NOT auto-installed browser MCP servers
INSERT OR IGNORE INTO mcp_server_registry (
    name, command, args, env, auto_launch, approved, notes, created_at, updated_at
) VALUES 
(
    'playwright',
    'npx',
    '["-y", "@modelcontextprotocol/server-playwright"]',
    '{}',
    0,
    0,
    'Official Playwright MCP server (Node.js required). Pre-vetted opt-in alternative for users desiring Playwright browser automation.',
    CURRENT_TIMESTAMP,
    CURRENT_TIMESTAMP
),
(
    'browser-use',
    'uvx',
    '["browser-use"]',
    '{}',
    0,
    0,
    'browser-use MCP server (Python/uv required). Pre-vetted opt-in alternative for users desiring browser-use automation.',
    CURRENT_TIMESTAMP,
    CURRENT_TIMESTAMP
);
