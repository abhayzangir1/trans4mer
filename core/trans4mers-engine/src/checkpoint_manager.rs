use rusqlite::Connection;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::execution::{Checkpoint, ExecutionState};
use trans4mers_domain::ids::ExecutionId;
use trans4mers_storage::repos::execution_repo;

pub struct CheckpointManager;

impl CheckpointManager {
    /// Saves the execution checkpoint to the database using the canonical upsert_checkpoint.
    pub fn save_checkpoint(conn: &Connection, cp: &Checkpoint) -> Result<(), Trans4mersError> {
        execution_repo::upsert_checkpoint(conn, cp)
    }

    /// Loads the latest checkpoint for an execution, validating generation.
    pub fn load_checkpoint(
        conn: &Connection,
        execution_id: &ExecutionId,
        expected_generation: u64,
    ) -> Result<Option<ExecutionState>, Trans4mersError> {
        if let Some(cp) = execution_repo::get_checkpoint(conn, execution_id)? {
            if cp.generation != expected_generation {
                return Err(Trans4mersError::StaleGeneration {
                    expected: expected_generation,
                    actual: cp.generation,
                });
            }
            if let Some(snapshot) = cp.context_snapshot {
                let state: ExecutionState = serde_json::from_value(snapshot)
                    .map_err(|e| Trans4mersError::Serialization(e.to_string()))?;
                Ok(Some(state))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    /// Prunes checkpoints older than the specified duration (in hours).
    pub fn prune_old_checkpoints(
        conn: &Connection,
        older_than_hours: i64,
    ) -> Result<usize, Trans4mersError> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::hours(older_than_hours)).to_rfc3339();
        let count = conn
            .execute(
                "DELETE FROM execution_checkpoints WHERE created_at < ?1",
                rusqlite::params![cutoff],
            )
            .map_err(|e| {
                Trans4mersError::Database(format!("Failed to prune old checkpoints: {}", e))
            })?;
        Ok(count)
    }
}
