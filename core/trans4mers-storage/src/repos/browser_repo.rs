use chrono::Utc;
use rusqlite::{Connection, params};
use trans4mers_domain::browser::{
    BrowserAuthEntry, BrowserPermissions, BrowserSnapshot, BrowserSpace,
};
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::ids::ProjectId;

pub fn create_space(conn: &Connection, space: &BrowserSpace) -> Result<(), Trans4mersError> {
    let perms_json = serde_json::to_string(&space.permissions)
        .map_err(|e| Trans4mersError::Serialization(e.to_string()))?;

    conn.execute(
        "INSERT INTO browser_spaces (
            id, project_id, name, profile_path, browser_binary, permissions, is_active, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            space.id,
            space.project_id.as_str(),
            space.name,
            space.profile_path,
            space.browser_binary,
            perms_json,
            if space.is_active { 1 } else { 0 },
            space.created_at.to_rfc3339(),
            space.updated_at.to_rfc3339(),
        ],
    ).map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}

pub fn get_space(conn: &Connection, id: &str) -> Result<Option<BrowserSpace>, Trans4mersError> {
    let mut stmt = conn.prepare(
        "SELECT id, project_id, name, profile_path, browser_binary, permissions, is_active, created_at, updated_at
         FROM browser_spaces WHERE id = ?1"
    ).map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut rows = stmt
        .query([id])
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    if let Some(row) = rows
        .next()
        .map_err(|e| Trans4mersError::Database(e.to_string()))?
    {
        Ok(Some(row_to_space(row)?))
    } else {
        Ok(None)
    }
}

pub fn list_spaces(
    conn: &Connection,
    project_id: &str,
) -> Result<Vec<BrowserSpace>, Trans4mersError> {
    let mut stmt = conn.prepare(
        "SELECT id, project_id, name, profile_path, browser_binary, permissions, is_active, created_at, updated_at
         FROM browser_spaces WHERE project_id = ?1 ORDER BY created_at ASC"
    ).map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let rows = stmt
        .query_map([project_id], row_to_space)
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut results = Vec::new();
    for r in rows {
        results.push(r.map_err(|e| Trans4mersError::Database(e.to_string()))?);
    }
    Ok(results)
}

pub fn update_space_active(
    conn: &Connection,
    id: &str,
    is_active: bool,
) -> Result<(), Trans4mersError> {
    conn.execute(
        "UPDATE browser_spaces SET is_active = ?1, updated_at = ?2 WHERE id = ?3",
        params![if is_active { 1 } else { 0 }, Utc::now().to_rfc3339(), id],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}

pub fn upsert_auth_entry(
    conn: &Connection,
    entry: &BrowserAuthEntry,
) -> Result<(), Trans4mersError> {
    let cookies_json = entry
        .cookies
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_default());
    let storage_json = entry
        .local_storage
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_default());
    let headers_json = entry
        .auth_headers
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_default());

    conn.execute(
        "INSERT INTO browser_auth_entries (
            id, space_id, service, cookies, local_storage, auth_headers, encrypted_blob, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        ON CONFLICT(space_id, service) DO UPDATE SET
            cookies = excluded.cookies,
            local_storage = excluded.local_storage,
            auth_headers = excluded.auth_headers,
            encrypted_blob = excluded.encrypted_blob,
            updated_at = excluded.updated_at",
        params![
            entry.id,
            entry.space_id,
            entry.service,
            cookies_json,
            storage_json,
            headers_json,
            entry.encrypted_blob,
            entry.created_at.to_rfc3339(),
            entry.updated_at.to_rfc3339(),
        ],
    ).map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}

pub fn list_auth_entries(
    conn: &Connection,
    space_id: &str,
) -> Result<Vec<BrowserAuthEntry>, Trans4mersError> {
    let mut stmt = conn.prepare(
        "SELECT id, space_id, service, cookies, local_storage, auth_headers, encrypted_blob, created_at, updated_at
         FROM browser_auth_entries WHERE space_id = ?1"
    ).map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let rows = stmt
        .query_map([space_id], row_to_auth)
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut results = Vec::new();
    for r in rows {
        results.push(r.map_err(|e| Trans4mersError::Database(e.to_string()))?);
    }
    Ok(results)
}

pub fn get_auth_entry(
    conn: &Connection,
    space_id: &str,
    service: &str,
) -> Result<Option<BrowserAuthEntry>, Trans4mersError> {
    let mut stmt = conn.prepare(
        "SELECT id, space_id, service, cookies, local_storage, auth_headers, encrypted_blob, created_at, updated_at
         FROM browser_auth_entries WHERE space_id = ?1 AND service = ?2"
    ).map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut rows = stmt
        .query(params![space_id, service])
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    if let Some(row) = rows
        .next()
        .map_err(|e| Trans4mersError::Database(e.to_string()))?
    {
        Ok(Some(row_to_auth(row)?))
    } else {
        Ok(None)
    }
}

pub fn insert_snapshot(
    conn: &Connection,
    snapshot: &BrowserSnapshot,
) -> Result<(), Trans4mersError> {
    conn.execute(
        "INSERT INTO browser_snapshots (
            id, space_id, reason, tree_hash, file_count, total_size_bytes, triggered_by, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            snapshot.id,
            snapshot.space_id,
            snapshot.reason,
            snapshot.tree_hash,
            snapshot.file_count as i64,
            snapshot.total_size_bytes as i64,
            snapshot.triggered_by,
            snapshot.created_at.to_rfc3339(),
        ],
    )
    .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    Ok(())
}

pub fn list_snapshots(
    conn: &Connection,
    space_id: &str,
) -> Result<Vec<BrowserSnapshot>, Trans4mersError> {
    let mut stmt = conn.prepare(
        "SELECT id, space_id, reason, tree_hash, file_count, total_size_bytes, triggered_by, created_at
         FROM browser_snapshots WHERE space_id = ?1 ORDER BY created_at DESC"
    ).map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let rows = stmt
        .query_map([space_id], |row| {
            let id: String = row.get(0)?;
            let space_id: String = row.get(1)?;
            let reason: Option<String> = row.get(2)?;
            let tree_hash: String = row.get(3)?;
            let file_count: i64 = row.get(4)?;
            let total_size_bytes: i64 = row.get(5)?;
            let triggered_by: Option<String> = row.get(6)?;
            let created_str: String = row.get(7)?;

            Ok(BrowserSnapshot {
                id,
                space_id,
                reason,
                tree_hash,
                file_count: file_count as usize,
                total_size_bytes: total_size_bytes as usize,
                triggered_by,
                created_at: crate::datetime_util::parse_db_datetime(&created_str),
            })
        })
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut results = Vec::new();
    for r in rows {
        results.push(r.map_err(|e| Trans4mersError::Database(e.to_string()))?);
    }
    Ok(results)
}

pub fn get_snapshot(
    conn: &Connection,
    snapshot_id: &str,
) -> Result<Option<BrowserSnapshot>, Trans4mersError> {
    let mut stmt = conn.prepare(
        "SELECT id, space_id, reason, tree_hash, file_count, total_size_bytes, triggered_by, created_at
         FROM browser_snapshots WHERE id = ?1"
    ).map_err(|e| Trans4mersError::Database(e.to_string()))?;

    let mut rows = stmt
        .query([snapshot_id])
        .map_err(|e| Trans4mersError::Database(e.to_string()))?;
    if let Some(row) = rows
        .next()
        .map_err(|e| Trans4mersError::Database(e.to_string()))?
    {
        let id: String = row.get(0)?;
        let space_id: String = row.get(1)?;
        let reason: Option<String> = row.get(2)?;
        let tree_hash: String = row.get(3)?;
        let file_count: i64 = row.get(4)?;
        let total_size_bytes: i64 = row.get(5)?;
        let triggered_by: Option<String> = row.get(6)?;
        let created_str: String = row.get(7)?;
        Ok(Some(BrowserSnapshot {
            id,
            space_id,
            reason,
            tree_hash,
            file_count: file_count as usize,
            total_size_bytes: total_size_bytes as usize,
            triggered_by,
            created_at: crate::datetime_util::parse_db_datetime(&created_str),
        }))
    } else {
        Ok(None)
    }
}

pub fn create_physical_snapshot(
    source_profile_dir: &std::path::Path,
    target_snapshot_dir: &std::path::Path,
) -> Result<(usize, usize, String), Trans4mersError> {
    if !source_profile_dir.exists() {
        std::fs::create_dir_all(source_profile_dir)
            .map_err(|e| Trans4mersError::Internal(e.to_string()))?;
    }
    if !target_snapshot_dir.exists() {
        std::fs::create_dir_all(target_snapshot_dir)
            .map_err(|e| Trans4mersError::Internal(e.to_string()))?;
    }

    let mut file_count = 0;
    let mut total_bytes = 0;
    let mut all_hashes = Vec::new();

    copy_dir_recursive(
        source_profile_dir,
        target_snapshot_dir,
        &mut file_count,
        &mut total_bytes,
        &mut all_hashes,
    )?;
    all_hashes.sort();
    let tree_hash = crate::filesystem::sha256_hex(all_hashes.join(",").as_bytes());

    Ok((file_count, total_bytes, tree_hash))
}

pub fn restore_physical_snapshot(
    source_snapshot_dir: &std::path::Path,
    target_profile_dir: &std::path::Path,
) -> Result<(), Trans4mersError> {
    if !source_snapshot_dir.exists() {
        return Err(Trans4mersError::Internal(format!(
            "Snapshot directory does not exist: {}",
            source_snapshot_dir.display()
        )));
    }
    if target_profile_dir.exists() {
        let _ = std::fs::remove_dir_all(target_profile_dir);
    }
    std::fs::create_dir_all(target_profile_dir)
        .map_err(|e| Trans4mersError::Internal(e.to_string()))?;

    let mut count = 0;
    let mut bytes = 0;
    let mut hashes = Vec::new();
    copy_dir_recursive(
        source_snapshot_dir,
        target_profile_dir,
        &mut count,
        &mut bytes,
        &mut hashes,
    )?;
    Ok(())
}

fn copy_dir_recursive(
    src: &std::path::Path,
    dst: &std::path::Path,
    file_count: &mut usize,
    total_bytes: &mut usize,
    hashes: &mut Vec<String>,
) -> Result<(), Trans4mersError> {
    if !src.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(src).map_err(|e| Trans4mersError::Internal(e.to_string()))? {
        let entry = entry.map_err(|e| Trans4mersError::Internal(e.to_string()))?;
        let file_type = entry
            .file_type()
            .map_err(|e| Trans4mersError::Internal(e.to_string()))?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if file_type.is_dir() {
            std::fs::create_dir_all(&dst_path)
                .map_err(|e| Trans4mersError::Internal(e.to_string()))?;
            copy_dir_recursive(&src_path, &dst_path, file_count, total_bytes, hashes)?;
        } else if file_type.is_file() {
            let data =
                std::fs::read(&src_path).map_err(|e| Trans4mersError::Internal(e.to_string()))?;
            *file_count += 1;
            *total_bytes += data.len();
            hashes.push(crate::filesystem::sha256_hex(&data));
            std::fs::write(&dst_path, &data)
                .map_err(|e| Trans4mersError::Internal(e.to_string()))?;
        }
    }
    Ok(())
}

fn row_to_space(row: &rusqlite::Row) -> rusqlite::Result<BrowserSpace> {
    let id: String = row.get(0)?;
    let proj_str: String = row.get(1)?;
    let name: String = row.get(2)?;
    let profile_path: String = row.get(3)?;
    let browser_binary: Option<String> = row.get(4)?;
    let perms_json: String = row.get(5)?;
    let is_active_int: i32 = row.get(6)?;
    let created_str: String = row.get(7)?;
    let updated_str: String = row.get(8)?;

    let permissions: BrowserPermissions = serde_json::from_str(&perms_json).unwrap_or_default();

    Ok(BrowserSpace {
        id,
        project_id: ProjectId::from_str(&proj_str).unwrap_or_default(),
        name,
        profile_path,
        browser_binary,
        permissions,
        is_active: is_active_int != 0,
        created_at: crate::datetime_util::parse_db_datetime(&created_str),
        updated_at: crate::datetime_util::parse_db_datetime(&updated_str),
    })
}

fn row_to_auth(row: &rusqlite::Row) -> rusqlite::Result<BrowserAuthEntry> {
    let id: String = row.get(0)?;
    let space_id: String = row.get(1)?;
    let service: String = row.get(2)?;
    let cookies_json: Option<String> = row.get(3)?;
    let storage_json: Option<String> = row.get(4)?;
    let headers_json: Option<String> = row.get(5)?;
    let encrypted_blob: Option<Vec<u8>> = row.get(6)?;
    let created_str: String = row.get(7)?;
    let updated_str: String = row.get(8)?;

    Ok(BrowserAuthEntry {
        id,
        space_id,
        service,
        cookies: cookies_json.and_then(|s| serde_json::from_str(&s).ok()),
        local_storage: storage_json.and_then(|s| serde_json::from_str(&s).ok()),
        auth_headers: headers_json.and_then(|s| serde_json::from_str(&s).ok()),
        encrypted_blob,
        created_at: crate::datetime_util::parse_db_datetime(&created_str),
        updated_at: crate::datetime_util::parse_db_datetime(&updated_str),
    })
}
