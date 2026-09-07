use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Once};
use trans4mers_domain::error::Trans4mersError;

/// Thread-safe database handle.
/// Readers spawn their own connections (WAL mode allows concurrent reads).
/// Writers lock the shared writer connection via Mutex and execute within a transaction.
#[derive(Clone)]
pub struct DbHandle {
    db_path: PathBuf,
    writer_conn: Arc<Mutex<Connection>>,
    reader_pool: Arc<Mutex<Vec<Connection>>>,
}

impl DbHandle {
    /// Installs sqlite-vec auto-extension once globally so all subsequently opened connections load vec0.
    pub fn install_sqlite_vec() {
        static ONCE: Once = Once::new();
        ONCE.call_once(|| unsafe {
            #[allow(clippy::missing_transmute_annotations)]
            let _ = rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
                sqlite_vec::sqlite3_vec_init as *const (),
            )));
        });
    }

    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, Trans4mersError> {
        Self::install_sqlite_vec();
        let db_path = path.as_ref().to_path_buf();

        let conn = Self::create_writer_connection(&db_path)?;

        Ok(Self {
            db_path,
            writer_conn: Arc::new(Mutex::new(conn)),
            reader_pool: Arc::new(Mutex::new(Vec::new())),
        })
    }

    /// Creates a new connection configured for exclusive writing.
    fn create_writer_connection(path: &Path) -> Result<Connection, Trans4mersError> {
        Self::install_sqlite_vec();
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| {
            Trans4mersError::Database(format!("Failed to open writer connection: {}", e))
        })?;

        Self::configure_connection(&conn, false)?;

        Ok(conn)
    }

    /// Creates a read-only connection.
    fn create_reader_connection(path: &Path) -> Result<Connection, Trans4mersError> {
        Self::install_sqlite_vec();
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| {
            Trans4mersError::Database(format!("Failed to open reader connection: {}", e))
        })?;

        Self::configure_connection(&conn, true)?;

        Ok(conn)
    }

    /// Configures SQLite connection pragmas.
    fn configure_connection(conn: &Connection, is_reader: bool) -> Result<(), Trans4mersError> {
        let pragma_query = if is_reader {
            "PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;
             PRAGMA query_only = ON;"
        } else {
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;"
        };

        conn.execute_batch(pragma_query)
            .map_err(|e| Trans4mersError::Database(format!("Failed to execute PRAGMAs: {}", e)))?;

        Ok(())
    }

    /// Executes a closure safely within a SQLite transaction using the exclusive writer lock.
    /// The transaction is automatically rolled back if an error is returned.
    pub fn with_write_tx<F, R>(&self, f: F) -> Result<R, Trans4mersError>
    where
        F: FnOnce(&rusqlite::Transaction) -> Result<R, Trans4mersError>,
    {
        let mut conn = self.writer_conn.lock().unwrap_or_else(|e| e.into_inner());

        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|e| {
                Trans4mersError::Database(format!("Failed to start transaction: {}", e))
            })?;

        let result = f(&tx)?;

        tx.commit().map_err(|e| {
            Trans4mersError::Database(format!("Failed to commit transaction: {}", e))
        })?;

        Ok(result)
    }

    /// Executes a closure using a pooled, safe, read-only connection.
    pub fn with_read_conn<F, R>(&self, f: F) -> Result<R, Trans4mersError>
    where
        F: FnOnce(&Connection) -> Result<R, Trans4mersError>,
    {
        let pooled_conn = {
            let mut pool = self.reader_pool.lock().unwrap_or_else(|e| e.into_inner());
            pool.pop()
        };

        let conn = match pooled_conn {
            Some(c) => c,
            None => Self::create_reader_connection(&self.db_path)?,
        };

        let result = f(&conn);

        let mut pool = self.reader_pool.lock().unwrap_or_else(|e| e.into_inner());
        if pool.len() < 16 {
            pool.push(conn);
        }

        result
    }

    /// Exposes an exclusive, mutable writer connection for schema migrations.
    pub fn with_exclusive_conn<F, R>(&self, f: F) -> Result<R, Trans4mersError>
    where
        F: FnOnce(&mut Connection) -> Result<R, Trans4mersError>,
    {
        let mut conn = self.writer_conn.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut conn)
    }

    /// Asynchronously executes a write transaction on Tokio's blocking thread pool,
    /// preventing blocking or starvation of the async worker threads during SQLite disk I/O.
    pub async fn with_write_tx_async<F, R>(&self, f: F) -> Result<R, Trans4mersError>
    where
        F: FnOnce(&rusqlite::Transaction) -> Result<R, Trans4mersError> + Send + 'static,
        R: Send + 'static,
    {
        let handle = self.clone();
        tokio::task::spawn_blocking(move || handle.with_write_tx(f))
            .await
            .map_err(|e| {
                Trans4mersError::Internal(format!("Tokio spawn_blocking panicked: {}", e))
            })?
    }

    /// Asynchronously executes a read operation on a dedicated read-only connection on Tokio's blocking thread pool.
    pub async fn with_read_conn_async<F, R>(&self, f: F) -> Result<R, Trans4mersError>
    where
        F: FnOnce(&Connection) -> Result<R, Trans4mersError> + Send + 'static,
        R: Send + 'static,
    {
        let handle = self.clone();
        tokio::task::spawn_blocking(move || handle.with_read_conn(f))
            .await
            .map_err(|e| {
                Trans4mersError::Internal(format!("Tokio spawn_blocking panicked: {}", e))
            })?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_db_handle_open_and_pool_reuse() {
        let unique = format!("t4m_db_{}.sqlite", uuid::Uuid::new_v4());
        let db_path = std::env::temp_dir().join(unique);

        let handle = DbHandle::open(&db_path).unwrap();

        // Perform write
        handle
            .with_write_tx(|tx| {
                tx.execute("CREATE TABLE test (id INTEGER PRIMARY KEY, val TEXT)", [])?;
                tx.execute("INSERT INTO test (id, val) VALUES (1, 'hello')", [])?;
                Ok(())
            })
            .unwrap();

        // Perform reads across pool
        for _ in 0..10 {
            let val: String = handle
                .with_read_conn(|conn| {
                    conn.query_row("SELECT val FROM test WHERE id = 1", [], |r| r.get(0))
                        .map_err(|e| Trans4mersError::Database(e.to_string()))
                })
                .unwrap();
            assert_eq!(val, "hello");
        }

        // Check pool has pooled connection
        assert!(!handle.reader_pool.lock().unwrap().is_empty());

        let _ = std::fs::remove_file(&db_path);
    }
}
