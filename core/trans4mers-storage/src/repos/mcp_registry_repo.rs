use chrono::Utc;
use rusqlite::{Connection, params};
use std::collections::HashMap;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::tool::McpServerEntry;

pub fn register_server(conn: &Connection, entry: &McpServerEntry) -> Result<(), Trans4mersError> {
    let args_json = serde_json::to_string(&entry.args)
        .map_err(|e| Trans4mersError::Serialization(e.to_string()))?;
    let env_json = serde_json::to_string(&entry.env)
        .map_err(|e| Trans4mersError::Serialization(e.to_string()))?;

    conn.execute(
        "INSERT INTO mcp_server_registry (
            name, command, args, env, auto_launch, approved, notes, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        ON CONFLICT(name) DO UPDATE SET
            command = excluded.command,
            args = excluded.args,
            env = excluded.env,
            auto_launch = excluded.auto_launch,
            approved = excluded.approved,
            notes = excluded.notes,
            updated_at = excluded.updated_at",
        params![
            entry.name,
            entry.command,
            args_json,
            env_json,
            if entry.auto_launch { 1 } else { 0 },
            if entry.approved { 1 } else { 0 },
            entry.notes,
            entry.created_at.to_rfc3339(),
            entry.updated_at.to_rfc3339(),
        ],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}

pub fn list_servers(conn: &Connection) -> Result<Vec<McpServerEntry>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT name, command, args, env, auto_launch, approved, notes, created_at, updated_at
         FROM mcp_server_registry
         ORDER BY name ASC",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let rows = stmt
        .query_map([], row_to_server)
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut results = Vec::new();
    for r in rows {
        results.push(r.map_err(|e| Trans4mersError::Database(e.to_string()))?);
    }
    Ok(results)
}

pub fn get_server(
    conn: &Connection,
    name: &str,
) -> Result<Option<McpServerEntry>, Trans4mersError> {
    let mut stmt = conn
        .prepare(
            "SELECT name, command, args, env, auto_launch, approved, notes, created_at, updated_at
         FROM mcp_server_registry WHERE name = ?1",
        )
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut rows = stmt
        .query([name])
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    if let Some(row) = rows
        .next()
        .map_err(|e| Trans4mersError::Database(e.to_string()))?
    {
        Ok(Some(row_to_server(row)?))
    } else {
        Ok(None)
    }
}

pub fn set_approval(conn: &Connection, name: &str, approved: bool) -> Result<(), Trans4mersError> {
    conn.execute(
        "UPDATE mcp_server_registry SET approved = ?1, updated_at = ?2 WHERE name = ?3",
        params![if approved { 1 } else { 0 }, Utc::now().to_rfc3339(), name],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}

pub fn set_auto_launch(
    conn: &Connection,
    name: &str,
    auto_launch: bool,
) -> Result<(), Trans4mersError> {
    conn.execute(
        "UPDATE mcp_server_registry SET auto_launch = ?1, updated_at = ?2 WHERE name = ?3",
        params![
            if auto_launch { 1 } else { 0 },
            Utc::now().to_rfc3339(),
            name
        ],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}

pub fn delete_server(conn: &Connection, name: &str) -> Result<(), Trans4mersError> {
    conn.execute("DELETE FROM mcp_server_registry WHERE name = ?1", [name])
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}

fn row_to_server(row: &rusqlite::Row) -> rusqlite::Result<McpServerEntry> {
    let name: String = row.get(0)?;
    let command: String = row.get(1)?;
    let args_json: String = row.get(2)?;
    let env_json: String = row.get(3)?;
    let auto_launch_int: i32 = row.get(4)?;
    let approved_int: i32 = row.get(5)?;
    let notes: Option<String> = row.get(6)?;
    let created_str: String = row.get(7)?;
    let updated_str: String = row.get(8)?;

    let args: Vec<String> = serde_json::from_str(&args_json).unwrap_or_default();
    let env: HashMap<String, String> = serde_json::from_str(&env_json).unwrap_or_default();

    Ok(McpServerEntry {
        name,
        command,
        args,
        env,
        auto_launch: auto_launch_int != 0,
        approved: approved_int != 0,
        notes,
        created_at: crate::datetime_util::parse_db_datetime(&created_str),
        updated_at: crate::datetime_util::parse_db_datetime(&updated_str),
    })
}
