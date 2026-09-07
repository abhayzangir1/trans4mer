-- Global schema M007 — MCP Protocol Traffic Logs
CREATE TABLE IF NOT EXISTS mcp_traffic_logs (
    id TEXT PRIMARY KEY,
    server_name TEXT NOT NULL,
    direction TEXT NOT NULL,
    method TEXT,
    payload TEXT NOT NULL,
    created_at DATETIME NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_mcp_traffic_logs_server ON mcp_traffic_logs(server_name, created_at ASC);
