use rusqlite::{Connection, params};
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::event::{McpTrafficDirection, McpTrafficFrame};

pub struct McpTrafficRepo;

impl McpTrafficRepo {
    pub fn insert(conn: &Connection, frame: &McpTrafficFrame) -> Result<(), Trans4mersError> {
        let dir_str = match frame.direction {
            McpTrafficDirection::Inbound => "inbound",
            McpTrafficDirection::Outbound => "outbound",
        };

        conn.execute(
            "INSERT INTO mcp_traffic_logs (id, server_name, direction, method, payload, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                frame.id,
                frame.server_name,
                dir_str,
                frame.method,
                frame.payload,
                frame.timestamp.to_rfc3339(),
            ],
        )
        .map_err(|e| {
            Trans4mersError::Database(format!("Failed to insert MCP traffic frame: {}", e))
        })?;

        Ok(())
    }

    pub fn list_by_server(
        conn: &Connection,
        server_name: &str,
        limit: u32,
    ) -> Result<Vec<McpTrafficFrame>, Trans4mersError> {
        let mut stmt = conn
            .prepare(
                "SELECT id, server_name, direction, method, payload, created_at
                 FROM mcp_traffic_logs
                 WHERE server_name = ?1
                 ORDER BY created_at ASC
                 LIMIT ?2",
            )
            .map_err(|e| Trans4mersError::Database(e.to_string()))?;

        let rows = stmt
            .query_map(params![server_name, limit], |row| {
                let id: String = row.get(0)?;
                let s_name: String = row.get(1)?;
                let dir_str: String = row.get(2)?;
                let method: Option<String> = row.get(3)?;
                let payload: String = row.get(4)?;
                let created_str: String = row.get(5)?;

                let direction = match dir_str.as_str() {
                    "inbound" => McpTrafficDirection::Inbound,
                    _ => McpTrafficDirection::Outbound,
                };

                let timestamp = crate::datetime_util::parse_db_datetime(&created_str);

                Ok(McpTrafficFrame {
                    id,
                    server_name: s_name,
                    direction,
                    method,
                    payload,
                    timestamp,
                })
            })
            .map_err(|e| Trans4mersError::Database(e.to_string()))?;

        let mut frames = Vec::new();
        for row in rows {
            frames.push(row.map_err(|e| Trans4mersError::Database(e.to_string()))?);
        }

        Ok(frames)
    }

    pub fn clear_by_server(conn: &Connection, server_name: &str) -> Result<(), Trans4mersError> {
        conn.execute(
            "DELETE FROM mcp_traffic_logs WHERE server_name = ?1",
            params![server_name],
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;
        Ok(())
    }

    pub fn list_recent(
        conn: &Connection,
        limit: u32,
    ) -> Result<Vec<McpTrafficFrame>, Trans4mersError> {
        let mut stmt = conn
            .prepare(
                "SELECT id, server_name, direction, method, payload, created_at
                 FROM mcp_traffic_logs
                 ORDER BY created_at DESC
                 LIMIT ?1",
            )
            .map_err(|e| Trans4mersError::Database(e.to_string()))?;

        let rows = stmt
            .query_map(params![limit], |row| {
                let id: String = row.get(0)?;
                let s_name: String = row.get(1)?;
                let dir_str: String = row.get(2)?;
                let method: Option<String> = row.get(3)?;
                let payload: String = row.get(4)?;
                let created_str: String = row.get(5)?;

                let direction = match dir_str.as_str() {
                    "inbound" => McpTrafficDirection::Inbound,
                    _ => McpTrafficDirection::Outbound,
                };

                let timestamp = crate::datetime_util::parse_db_datetime(&created_str);

                Ok(McpTrafficFrame {
                    id,
                    server_name: s_name,
                    direction,
                    method,
                    payload,
                    timestamp,
                })
            })
            .map_err(|e| Trans4mersError::Database(e.to_string()))?;

        let mut frames = Vec::new();
        for row in rows {
            frames.push(row.map_err(|e| Trans4mersError::Database(e.to_string()))?);
        }

        frames.reverse();
        Ok(frames)
    }
}
