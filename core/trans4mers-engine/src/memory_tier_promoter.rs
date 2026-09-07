use crate::app_state::AppState;
use chrono::{Duration, Utc};
use std::sync::Arc;
use tracing::info;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::event::DomainEvent;
use trans4mers_domain::ids::{ActorId, ProjectId};
use trans4mers_domain::memory::MemoryTier;

pub struct MemoryTierPromoter;

impl MemoryTierPromoter {
    /// Evaluates memories within a project and promotes/prunes them across the 4-tier pyramid.
    pub async fn run_promotion_cycle(
        app_state: &AppState,
        project_id: &ProjectId,
    ) -> Result<usize, Trans4mersError> {
        let db = match app_state.get_project_db(project_id) {
            Some(d) => d,
            None => return Ok(0),
        };

        let mut promoted_count = 0;
        let now = Utc::now();
        let working_cutoff = (now - Duration::minutes(10)).to_rfc3339();

        // 1. Working -> Episodic:
        // Working memories older than 10 mins are promoted to Episodic
        let mut working_to_promote: Vec<String> = Vec::new();
        db.with_read_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT id FROM project_memories
                 WHERE project_id = ?1 AND tier = 'Working' AND created_at < ?2",
                )
                .map_err(|e| Trans4mersError::Database(e.to_string()))?;

            let rows = stmt
                .query_map(
                    rusqlite::params![project_id.as_str(), working_cutoff],
                    |row| row.get::<_, String>(0),
                )
                .map_err(|e| Trans4mersError::Database(e.to_string()))?;

            for r in rows.flatten() {
                working_to_promote.push(r);
            }
            Ok(())
        })?;

        for mem_id in working_to_promote {
            Self::promote_memory(
                app_state,
                project_id,
                &mem_id,
                MemoryTier::Working,
                MemoryTier::Episodic,
            )
            .await?;
            promoted_count += 1;
        }

        // 2. Episodic -> Semantic:
        // Episodic memories retrieved >= 3 times with importance >= 0.6
        let mut episodic_to_promote: Vec<String> = Vec::new();
        db.with_read_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id FROM project_memories
                 WHERE project_id = ?1 AND tier = 'Episodic' AND retrieval_count >= 3 AND importance >= 0.6"
            ).map_err(|e| Trans4mersError::Database(e.to_string()))?;

            let rows = stmt.query_map(rusqlite::params![project_id.as_str()], |row| row.get::<_, String>(0))
                .map_err(|e| Trans4mersError::Database(e.to_string()))?;

            for r in rows.flatten() {
                episodic_to_promote.push(r);
            }
            Ok(())
        })?;

        for mem_id in episodic_to_promote {
            Self::promote_memory(
                app_state,
                project_id,
                &mem_id,
                MemoryTier::Episodic,
                MemoryTier::Semantic,
            )
            .await?;
            promoted_count += 1;
        }

        // 3. Semantic -> Procedural:
        // Semantic memories with retrieval_count >= 8 and importance >= 0.8
        let mut semantic_to_promote: Vec<String> = Vec::new();
        db.with_read_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id FROM project_memories
                 WHERE project_id = ?1 AND tier = 'Semantic' AND (retrieval_count >= 8 OR depth_level >= 2) AND importance >= 0.75"
            ).map_err(|e| Trans4mersError::Database(e.to_string()))?;

            let rows = stmt.query_map(rusqlite::params![project_id.as_str()], |row| row.get::<_, String>(0))
                .map_err(|e| Trans4mersError::Database(e.to_string()))?;

            for r in rows.flatten() {
                semantic_to_promote.push(r);
            }
            Ok(())
        })?;

        for mem_id in semantic_to_promote {
            Self::promote_memory(
                app_state,
                project_id,
                &mem_id,
                MemoryTier::Semantic,
                MemoryTier::Procedural,
            )
            .await?;
            promoted_count += 1;
        }

        // 4. Prune low-importance stale episodic memories (> 14 days old, 0 retrievals, importance < 0.25)
        let stale_cutoff = (now - Duration::days(14)).to_rfc3339();
        let _ = db.with_write_tx(|conn| {
            conn.execute(
                "DELETE FROM project_memories
                 WHERE project_id = ?1 AND tier = 'Episodic' AND retrieval_count = 0 AND importance < 0.25 AND created_at < ?2",
                rusqlite::params![project_id.as_str(), stale_cutoff],
            ).map_err(|e| Trans4mersError::Database(e.to_string()))
        });

        if promoted_count > 0 {
            info!(project_id = %project_id, count = promoted_count, "Promoted memories across pyramid tiers");
        }

        Ok(promoted_count)
    }

    async fn promote_memory(
        app_state: &AppState,
        project_id: &ProjectId,
        memory_id: &str,
        old_tier: MemoryTier,
        new_tier: MemoryTier,
    ) -> Result<(), Trans4mersError> {
        let db = app_state
            .get_project_db(project_id)
            .ok_or_else(|| Trans4mersError::Database("Project DB not found".to_string()))?;

        let event = DomainEvent::MemoryTierPromoted {
            project_id: *project_id,
            memory_id: memory_id.to_string(),
            old_tier: old_tier.to_string(),
            new_tier: new_tier.to_string(),
        };

        let envelope = db.with_write_tx(|tx| {
            tx.execute(
                "UPDATE project_memories SET tier = ?1, updated_at = ?2 WHERE id = ?3 AND project_id = ?4",
                rusqlite::params![
                    new_tier.to_string(),
                    chrono::Utc::now().to_rfc3339(),
                    memory_id,
                    project_id.as_str()
                ],
            ).map_err(|e| Trans4mersError::Database(e.to_string()))?;

            crate::cqrs::commit_event(
                tx,
                event,
                ActorId::new(),
            )
        })?;

        let env_arc = Arc::new(envelope);
        let _ = app_state.get_event_bus(project_id).publish(env_arc.clone());
        let _ = app_state.global_event_bus.publish(env_arc);

        Ok(())
    }
}
