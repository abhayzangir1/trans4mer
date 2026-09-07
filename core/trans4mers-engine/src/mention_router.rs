use rusqlite::params;
use tracing::debug;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::ids::{AgentInstanceId, ProjectId};
use trans4mers_storage::DbHandle;

pub struct MentionRouter;

#[derive(Debug, Clone)]
pub struct ResolvedMention {
    pub definition_id: String,
    pub agent_instance_id: Option<AgentInstanceId>,
}

impl MentionRouter {
    /// Scans a text payload for `@AgentName` patterns, resolves them to definition IDs
    /// and any existing AgentInstanceIds in the project.
    pub fn extract_mention_targets(
        global_db: &DbHandle,
        project_db: &DbHandle,
        project_id: &ProjectId,
        text: &str,
    ) -> Result<Vec<ResolvedMention>, Trans4mersError> {
        let mut results = Vec::new();

        let potential_names: Vec<&str> = text
            .split_whitespace()
            .filter(|word| word.starts_with('@') && word.len() > 1)
            .map(|word| {
                word.trim_start_matches('@')
                    .trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
            })
            .collect();

        if potential_names.is_empty() {
            return Ok(results);
        }

        debug!("Found potential mentions: {:?}", potential_names);

        for name in potential_names {
            // 1. Resolve name to Definition ID in global_db (matches id, name, or role)
            let def_id: Option<String> = global_db
                .with_read_conn(|conn| {
                    let mut stmt = conn
                        .prepare(
                            "SELECT id FROM agent_definitions
                     WHERE name = ?1 COLLATE NOCASE
                        OR id = ?1 COLLATE NOCASE
                        OR role LIKE '%' || ?1 || '%' COLLATE NOCASE
                     LIMIT 1",
                        )
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                    let res = stmt.query_row([name], |row| row.get(0)).ok();
                    Ok(res)
                })
                .map_err(|e| Trans4mersError::Internal(e.to_string()))?;

            if let Some(def_uuid) = def_id {
                // 2. Check if instance exists in project_db
                let mut found_inst: Option<AgentInstanceId> = None;
                project_db.with_read_conn(|conn| {
                    let mut stmt = conn.prepare(
                        "SELECT id FROM agent_instances WHERE project_id = ?1 AND definition_id = ?2 AND status != 'TERMINATED' ORDER BY created_at ASC LIMIT 1"
                    ).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                    let row_res: Result<String, _> = stmt.query_row(params![project_id.as_str(), def_uuid], |row| row.get(0));
                    if let Ok(id_str) = row_res
                        && let Ok(inst_id) = std::str::FromStr::from_str(&id_str) {
                            found_inst = Some(inst_id);
                        }
                    Ok(())
                }).map_err(|e| Trans4mersError::Internal(e.to_string()))?;

                results.push(ResolvedMention {
                    definition_id: def_uuid,
                    agent_instance_id: found_inst,
                });
            }
        }

        Ok(results)
    }

    /// Legacy / simple wrapper returning only existing instance IDs.
    pub fn extract_and_resolve_mentions(
        global_db: &DbHandle,
        project_db: &DbHandle,
        project_id: &ProjectId,
        text: &str,
    ) -> Result<Vec<AgentInstanceId>, Trans4mersError> {
        let targets = Self::extract_mention_targets(global_db, project_db, project_id, text)?;
        Ok(targets
            .into_iter()
            .filter_map(|t| t.agent_instance_id)
            .collect())
    }
}
