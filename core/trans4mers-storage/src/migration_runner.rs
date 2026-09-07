use chrono::Utc;
use rusqlite::Connection;
use tracing::info;
use trans4mers_domain::error::Trans4mersError;

pub struct MigrationRunner;

impl MigrationRunner {
    pub fn run_global_migrations(conn: &mut Connection) -> Result<(), Trans4mersError> {
        let tx = conn
            .transaction()
            .map_err(|e| Trans4mersError::Migration(e.to_string()))?;

        tx.execute(
            "CREATE TABLE IF NOT EXISTS _schema_migrations (
                version TEXT PRIMARY KEY,
                applied_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )
        .map_err(|e| Trans4mersError::Migration(e.to_string()))?;

        Self::apply_migration(
            &tx,
            "M001__init",
            include_str!("migrations/global/M001__init.sql"),
        )?;

        Self::apply_migration(
            &tx,
            "M002__seed_agents",
            include_str!("migrations/global/M002__seed_agents.sql"),
        )?;

        Self::apply_migration(
            &tx,
            "M003__cost_and_mcp_registry",
            include_str!("migrations/global/M003__cost_and_mcp_registry.sql"),
        )?;

        Self::apply_migration(
            &tx,
            "M004__seed_browser_mcp",
            include_str!("migrations/global/M004__seed_browser_mcp.sql"),
        )?;

        Self::apply_migration(
            &tx,
            "M005__fts5_global_search",
            include_str!("migrations/global/M005__fts5_global_search.sql"),
        )?;

        Self::apply_migration(
            &tx,
            "M006__global_embeddings",
            include_str!("migrations/global/M006__global_embeddings.sql"),
        )?;

        Self::apply_migration(
            &tx,
            "M007__mcp_traffic_logs",
            include_str!("migrations/global/M007__mcp_traffic_logs.sql"),
        )?;

        Self::apply_migration(
            &tx,
            "M008__seed_github_mcp",
            include_str!("migrations/global/M008__seed_github_mcp.sql"),
        )?;

        Self::apply_migration(
            &tx,
            "M009__seed_default_policies",
            include_str!("migrations/global/M009__seed_default_policies.sql"),
        )?;

        tx.commit()
            .map_err(|e| Trans4mersError::Migration(e.to_string()))?;
        Ok(())
    }

    pub fn run_project_migrations(
        conn: &mut Connection,
        vector_dimensions: u32,
    ) -> Result<(), Trans4mersError> {
        let tx = conn
            .transaction()
            .map_err(|e| Trans4mersError::Migration(e.to_string()))?;

        tx.execute(
            "CREATE TABLE IF NOT EXISTS _schema_migrations (
                version TEXT PRIMARY KEY,
                applied_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )
        .map_err(|e| Trans4mersError::Migration(e.to_string()))?;

        Self::apply_migration(
            &tx,
            "M001__init",
            include_str!("migrations/project/M001__init.sql"),
        )?;

        Self::apply_migration(
            &tx,
            "M002__locks_and_approvals",
            include_str!("migrations/project/M002__locks_and_approvals.sql"),
        )?;

        // Generate sqlite-vec dimension-locked tables
        let vec_migration = Self::generate_vector_tables_migration(vector_dimensions);
        Self::apply_migration(&tx, "M003__vector_tables", &vec_migration)?;

        Self::apply_migration(
            &tx,
            "M004__memory_pyramid_and_diffs",
            include_str!("migrations/project/M004__memory_pyramid_and_diffs.sql"),
        )?;

        Self::apply_migration(
            &tx,
            "M005__document_rag",
            include_str!("migrations/project/M005__document_rag.sql"),
        )?;

        let vec_doc_migration = format!(
            "CREATE VIRTUAL TABLE IF NOT EXISTS vec_doc_chunks USING vec0(embedding float[{vector_dimensions}]);"
        );
        Self::apply_migration(&tx, "M006__vec_doc_chunks", &vec_doc_migration)?;

        Self::apply_migration(
            &tx,
            "M007__interactive_artifacts_and_schedules",
            include_str!("migrations/project/M007__interactive_artifacts_and_schedules.sql"),
        )?;

        Self::apply_migration(
            &tx,
            "M008__domain_models",
            include_str!("migrations/project/M008__domain_models.sql"),
        )?;

        Self::apply_migration(
            &tx,
            "M009__fts5_hybrid_rag",
            include_str!("migrations/project/M009__fts5_hybrid_rag.sql"),
        )?;

        Self::apply_migration(
            &tx,
            "M010__vector_store_blobs_and_settings",
            include_str!("migrations/project/M010__vector_store_blobs_and_settings.sql"),
        )?;

        Self::apply_migration(
            &tx,
            "M011__execution_created_at_and_diff_updated_at",
            include_str!("migrations/project/M011__execution_created_at_and_diff_updated_at.sql"),
        )?;

        Self::apply_migration(
            &tx,
            "M012__memory_temporal_validity",
            include_str!("migrations/project/M012__memory_temporal_validity.sql"),
        )?;

        tx.commit()
            .map_err(|e| Trans4mersError::Migration(e.to_string()))?;
        Ok(())
    }

    fn apply_migration(
        tx: &rusqlite::Transaction,
        version: &str,
        sql: &str,
    ) -> Result<(), Trans4mersError> {
        let count: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM _schema_migrations WHERE version = ?1",
                [version],
                |row| row.get(0),
            )
            .map_err(|e| Trans4mersError::Migration(e.to_string()))?;

        if count == 0 {
            info!("Applying migration {}...", version);
            tx.execute_batch(sql).map_err(|e| {
                Trans4mersError::Migration(format!("Migration {} failed: {}", version, e))
            })?;

            tx.execute(
                "INSERT INTO _schema_migrations (version, applied_at) VALUES (?1, ?2)",
                (version, Utc::now().to_rfc3339()),
            )
            .map_err(|e| {
                Trans4mersError::Migration(format!("Failed to record migration {}: {}", version, e))
            })?;
        }

        Ok(())
    }

    fn generate_vector_tables_migration(dimensions: u32) -> String {
        format!(
            "
            CREATE TABLE IF NOT EXISTS vec_id_map (
                rowid INTEGER PRIMARY KEY AUTOINCREMENT,
                uuid TEXT UNIQUE NOT NULL
            );
            CREATE VIRTUAL TABLE IF NOT EXISTS vec_learned_rules USING vec0(
                embedding float[{0}]
            );
            CREATE VIRTUAL TABLE IF NOT EXISTS vec_project_memories USING vec0(
                embedding float[{0}]
            );
            ",
            dimensions
        )
    }
}
