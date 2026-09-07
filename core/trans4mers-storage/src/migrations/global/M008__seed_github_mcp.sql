-- Global schema M008 — Seed pre-vetted, NOT auto-installed GitHub MCP server
INSERT OR IGNORE INTO mcp_server_registry (
    name, command, args, env, auto_launch, approved, notes, created_at, updated_at
) VALUES (
    'github',
    'npx',
    '["-y", "@modelcontextprotocol/server-github"]',
    '{}',
    0,
    0,
    'Official GitHub MCP server (Node.js required). Pre-vetted opt-in alternative. Personal Access Token (PAT) stored in OS Keyring.',
    CURRENT_TIMESTAMP,
    CURRENT_TIMESTAMP
);
