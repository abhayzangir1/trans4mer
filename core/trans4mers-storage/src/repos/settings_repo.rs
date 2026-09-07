use rusqlite::{Connection, OptionalExtension, params};
use trans4mers_domain::error::Trans4mersError;

/// Reads a project-level setting value by key.
pub fn get_project_setting(
    conn: &Connection,
    key: &str,
) -> Result<Option<String>, Trans4mersError> {
    let mut stmt = conn
        .prepare("SELECT value FROM project_settings WHERE key = ?1")
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let val = stmt
        .query_row(params![key], |row| row.get(0))
        .optional()
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    Ok(val)
}

/// Sets or updates a project-level setting key-value pair.
pub fn set_project_setting(
    conn: &Connection,
    key: &str,
    value: &str,
) -> Result<(), Trans4mersError> {
    conn.execute(
        "INSERT INTO project_settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}

/// Returns the active vector backend for this project ("sqlite-vec" or "lancedb").
/// Defaults to "sqlite-vec" if not explicitly configured.
pub fn get_vector_backend(conn: &Connection) -> Result<String, Trans4mersError> {
    let setting = get_project_setting(conn, "vector_backend")?;
    Ok(setting.unwrap_or_else(|| "sqlite-vec".to_string()))
}

/// Updates the active vector backend for this project ("sqlite-vec" or "lancedb").
pub fn set_vector_backend(conn: &Connection, backend: &str) -> Result<(), Trans4mersError> {
    set_project_setting(conn, "vector_backend", backend)
}

/// Reads a global app-level setting value by key from global.sqlite app_settings table.
pub fn get_app_setting(conn: &Connection, key: &str) -> Result<Option<String>, Trans4mersError> {
    let mut stmt = conn
        .prepare("SELECT value FROM app_settings WHERE key = ?1")
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let val = stmt
        .query_row(params![key], |row| row.get(0))
        .optional()
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    Ok(val)
}

/// Sets or updates a global app-level setting key-value pair in global.sqlite app_settings table.
pub fn set_app_setting(conn: &Connection, key: &str, value: &str) -> Result<(), Trans4mersError> {
    conn.execute(
        "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}
