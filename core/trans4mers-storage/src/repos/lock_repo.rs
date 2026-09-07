use crate::db_handle::DbHandle;
use chrono::Utc;
use rusqlite::params;
use std::sync::Arc;
use trans4mers_domain::error::Trans4mersError;

pub struct LockRepo {
    db: Arc<DbHandle>,
}

impl LockRepo {
    pub fn new(db: Arc<DbHandle>) -> Self {
        Self { db }
    }

    /// Try to acquire an advisory lock. Succeeds when free or when the existing lock
    /// has expired (stolen). Returns true if acquired, false if held by another owner.
    pub fn try_acquire(
        &self,
        key: &str,
        holder: &str,
        ttl_secs: i64,
    ) -> Result<bool, Trans4mersError> {
        self.db.with_write_tx(|tx| {
            let now = Utc::now().to_rfc3339();
            let expires_at = (Utc::now() + chrono::Duration::seconds(ttl_secs)).to_rfc3339();

            // Purge expired locks first
            tx.execute("DELETE FROM locks WHERE expires_at < ?1", params![now])
                .map_err(|e| Trans4mersError::Database(format!("lock purge failed: {}", e)))?;

            let n = tx
                .execute(
                    "INSERT INTO locks (id, lock_key, holder, expires_at, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(lock_key) DO UPDATE SET expires_at = excluded.expires_at
                 WHERE locks.holder = excluded.holder",
                    params![
                        uuid::Uuid::new_v4().to_string(),
                        key,
                        holder,
                        expires_at,
                        now,
                    ],
                )
                .map_err(|e| Trans4mersError::Database(format!("lock acquire failed: {}", e)))?;

            Ok(n > 0)
        })
    }

    /// Renew an existing lock held by the given owner, extending its TTL.
    pub fn renew(&self, key: &str, holder: &str, ttl_secs: i64) -> Result<bool, Trans4mersError> {
        self.db.with_write_tx(|tx| {
            let expires_at = (Utc::now() + chrono::Duration::seconds(ttl_secs)).to_rfc3339();
            let n = tx
                .execute(
                    "UPDATE locks SET expires_at = ?1 WHERE lock_key = ?2 AND holder = ?3",
                    params![expires_at, key, holder],
                )
                .map_err(|e| Trans4mersError::Database(format!("lock renew failed: {}", e)))?;
            Ok(n > 0)
        })
    }

    /// Release a lock held by the given owner (idempotent).
    pub fn release(&self, key: &str, holder: &str) -> Result<(), Trans4mersError> {
        self.db.with_write_tx(|tx| {
            tx.execute(
                "DELETE FROM locks WHERE lock_key = ?1 AND holder = ?2",
                params![key, holder],
            )
            .map_err(|e| Trans4mersError::Database(format!("lock release failed: {}", e)))?;
            Ok(())
        })
    }

    /// Holder of an active lock, if any.
    pub fn holder(&self, key: &str) -> Result<Option<String>, Trans4mersError> {
        self.db.with_read_conn(|c| {
            let now = Utc::now().to_rfc3339();
            let mut stmt = c
                .prepare("SELECT holder FROM locks WHERE lock_key = ?1 AND expires_at >= ?2")
                .map_err(|e| Trans4mersError::Database(format!("lock query failed: {}", e)))?;
            let mut rows = stmt
                .query(params![key, now])
                .map_err(|e| Trans4mersError::Database(format!("lock query failed: {}", e)))?;
            if let Some(row) = rows
                .next()
                .map_err(|e| Trans4mersError::Database(format!("lock query failed: {}", e)))?
            {
                Ok(Some(row.get(0).map_err(|e| {
                    Trans4mersError::Database(format!("lock query failed: {}", e))
                })?))
            } else {
                Ok(None)
            }
        })
    }

    /// Purge all expired locks from the table. Returns number of locks purged.
    pub fn purge_expired(&self) -> Result<usize, Trans4mersError> {
        self.db.with_write_tx(|tx| {
            let now = Utc::now().to_rfc3339();
            let count = tx
                .execute("DELETE FROM locks WHERE expires_at < ?1", params![now])
                .map_err(|e| Trans4mersError::Database(format!("lock purge failed: {}", e)))?;
            Ok(count)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration_runner::MigrationRunner;

    #[test]
    fn test_lock_acquire_contention_and_release() {
        let unique = format!("t4m_lock_test_{}.sqlite", uuid::Uuid::new_v4());
        let db_path = std::env::temp_dir().join(unique);
        let db = Arc::new(DbHandle::open(&db_path).unwrap());

        db.with_exclusive_conn(|conn| MigrationRunner::run_project_migrations(conn, 384))
            .unwrap();

        let repo = LockRepo::new(db.clone());
        let key = "file:test_project:src/main.rs";

        // 1. Agent 1 acquires
        assert!(repo.try_acquire(key, "agent-1", 60).unwrap());

        // 1b. Agent 1 renews via try_acquire and explicit renew
        assert!(repo.try_acquire(key, "agent-1", 120).unwrap());
        assert!(repo.renew(key, "agent-1", 180).unwrap());

        // 2. Agent 2 attempts acquisition and is blocked
        assert!(!repo.try_acquire(key, "agent-2", 60).unwrap());
        assert!(!repo.renew(key, "agent-2", 60).unwrap());

        // 3. Verify holder
        assert_eq!(repo.holder(key).unwrap(), Some("agent-1".to_string()));

        // 4. Agent 1 releases
        repo.release(key, "agent-1").unwrap();

        // 5. Agent 2 can now acquire
        assert!(repo.try_acquire(key, "agent-2", 60).unwrap());
        assert_eq!(repo.holder(key).unwrap(), Some("agent-2".to_string()));

        let _ = std::fs::remove_file(&db_path);
    }
}
