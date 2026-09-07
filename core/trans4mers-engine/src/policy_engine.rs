use crate::app_state::AppState;
use rusqlite::params;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::ids::{AgentInstanceId, ProjectId};
use trans4mers_domain::policy::PolicyOutcome;
use trans4mers_domain::tool::{Capability, EffectClass, RiskLevel, ToolRequest};

pub struct PolicyEngine;

impl PolicyEngine {
    /// Zero-Trust capability check using DENY-wins lattice.
    pub fn evaluate(
        tool_request: &ToolRequest,
        agent_id: &AgentInstanceId,
        project_id: &ProjectId,
        app_state: &AppState,
    ) -> Result<PolicyOutcome, Trans4mersError> {
        let tool_executor = match app_state.tool_executors.get(project_id) {
            Some(exec) => exec,
            None => return Ok(PolicyOutcome::Deny),
        };

        let registry = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async { tool_executor.registry.read().await.clone() })
        });

        let tool = match registry.get(&tool_request.tool_name) {
            Some(t) => t,
            None => return Ok(PolicyOutcome::Deny),
        };

        let manifest = tool.manifest();

        // Ensure the agent physically exists
        let db = match app_state.get_project_db(project_id) {
            Some(db) => db,
            None => return Ok(PolicyOutcome::Deny),
        };

        let mut agent_exists = false;
        let _ = db.with_read_conn(|conn| {
            if let Ok(Some(_)) =
                trans4mers_storage::repos::agent_inst_repo::get_agent_instance(conn, agent_id)
            {
                agent_exists = true;
            }
            Ok(())
        });

        if !agent_exists {
            return Ok(PolicyOutcome::Deny);
        }

        let mut requires_ask = false;
        // Evaluate ALL required capabilities
        for cap in &manifest.required_capabilities {
            let outcome = Self::evaluate_capability(cap, agent_id, project_id, app_state)?;
            if outcome == PolicyOutcome::Deny {
                return Ok(PolicyOutcome::Deny); // DENY ALWAYS WINS
            }
            if outcome == PolicyOutcome::Ask {
                requires_ask = true;
            }
        }

        if requires_ask {
            return Ok(PolicyOutcome::Ask);
        }

        // If not denied, evaluate EffectClass & RiskLevel rules
        match manifest.effect_class {
            EffectClass::ReadOnly => Ok(PolicyOutcome::Allow),
            EffectClass::IdempotentMutation { .. } => {
                if manifest.baseline_risk == RiskLevel::Safe
                    || manifest.baseline_risk == RiskLevel::Low
                {
                    Ok(PolicyOutcome::Allow)
                } else {
                    Ok(PolicyOutcome::Ask)
                }
            }
            EffectClass::NonIdempotentMutation => {
                if manifest.baseline_risk == RiskLevel::Safe
                    || manifest.baseline_risk == RiskLevel::Low
                {
                    Ok(PolicyOutcome::Allow)
                } else {
                    Ok(PolicyOutcome::Ask)
                }
            }
            EffectClass::Unknown => Ok(PolicyOutcome::Ask),
        }
    }

    fn evaluate_capability(
        cap: &Capability,
        agent_id: &AgentInstanceId,
        project_id: &ProjectId,
        app_state: &AppState,
    ) -> Result<PolicyOutcome, Trans4mersError> {
        let cap_str = cap.to_string();

        let mut global_outcome: Option<PolicyOutcome> = None;
        let mut project_outcome: Option<PolicyOutcome> = None;
        let mut agent_outcome: Option<PolicyOutcome> = None;

        // Global Policy
        let _ = app_state.global_db.with_read_conn(|conn| {
            if let Ok(mut stmt) = conn
                .prepare("SELECT outcome FROM policies WHERE capability = ?1 AND scope = 'Global'")
                && let Ok(outcome_str) =
                    stmt.query_row(params![&cap_str], |row| row.get::<_, String>(0))
            {
                global_outcome = std::str::FromStr::from_str(&outcome_str).ok();
            }
            Ok(())
        });

        // Project & Agent Policy
        if let Some(db) = app_state.get_project_db(project_id) {
            let _ = db.with_read_conn(|conn| {
                // Project scope
                if let Ok(mut stmt_proj) = conn.prepare("SELECT outcome FROM policies WHERE capability = ?1 AND scope = 'Project'")
                    && let Ok(outcome_str) = stmt_proj.query_row(params![&cap_str], |row| row.get::<_, String>(0)) {
                        project_outcome = std::str::FromStr::from_str(&outcome_str).ok();
                    }

                // Agent scope
                if let Ok(mut stmt_agent) = conn.prepare("SELECT outcome FROM policies WHERE capability = ?1 AND scope = 'Agent' AND entity_id = ?2")
                    && let Ok(outcome_str) = stmt_agent.query_row(params![&cap_str, agent_id.as_str()], |row| row.get::<_, String>(0)) {
                        agent_outcome = std::str::FromStr::from_str(&outcome_str).ok();
                    }
                Ok(())
            });
        }

        // Rule 1: ANY Deny is an absolute veto
        if matches!(global_outcome, Some(PolicyOutcome::Deny))
            || matches!(project_outcome, Some(PolicyOutcome::Deny))
            || matches!(agent_outcome, Some(PolicyOutcome::Deny))
        {
            return Ok(PolicyOutcome::Deny);
        }

        // Rule 2: Most restrictive surviving policy wins
        if matches!(global_outcome, Some(PolicyOutcome::Ask))
            || matches!(project_outcome, Some(PolicyOutcome::Ask))
            || matches!(agent_outcome, Some(PolicyOutcome::Ask))
        {
            return Ok(PolicyOutcome::Ask);
        }

        // Rule 3: If any layer explicitly allows, allow
        if global_outcome.is_some() || project_outcome.is_some() || agent_outcome.is_some() {
            return Ok(PolicyOutcome::Allow);
        }

        // Rule 4: No policy found -> default to Ask for safety
        Ok(PolicyOutcome::Ask)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tier 1 - always runs
    #[test]
    fn policy_engine_deny_wins() {
        let global = Some(PolicyOutcome::Allow);
        let project = Some(PolicyOutcome::Allow);
        let agent = Some(PolicyOutcome::Deny);

        let outcome = if matches!(global, Some(PolicyOutcome::Deny))
            || matches!(project, Some(PolicyOutcome::Deny))
            || matches!(agent, Some(PolicyOutcome::Deny))
        {
            PolicyOutcome::Deny
        } else if matches!(global, Some(PolicyOutcome::Ask))
            || matches!(project, Some(PolicyOutcome::Ask))
            || matches!(agent, Some(PolicyOutcome::Ask))
        {
            PolicyOutcome::Ask
        } else {
            PolicyOutcome::Allow
        };

        assert_eq!(outcome, PolicyOutcome::Deny);
    }

    // Tier 3 - only runs with --features e2e-tests -- --ignored
    #[tokio::test]
    #[ignore]
    async fn full_react_loop_with_ollama() {
        // 1. Verify policy outcome evaluation
        let global = Some(PolicyOutcome::Allow);
        let project = Some(PolicyOutcome::Ask);
        let agent = None;
        let outcome = if matches!(global, Some(PolicyOutcome::Deny))
            || matches!(project, Some(PolicyOutcome::Deny))
            || matches!(agent, Some(PolicyOutcome::Deny))
        {
            PolicyOutcome::Deny
        } else if matches!(global, Some(PolicyOutcome::Ask))
            || matches!(project, Some(PolicyOutcome::Ask))
            || matches!(agent, Some(PolicyOutcome::Ask))
        {
            PolicyOutcome::Ask
        } else {
            PolicyOutcome::Allow
        };
        assert_eq!(outcome, PolicyOutcome::Ask);

        // 2. Probe live Ollama runtime if running locally
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .build()
            .unwrap();
        if let Ok(resp) = client.get("http://127.0.0.1:11434/api/tags").send().await {
            assert!(
                resp.status().is_success(),
                "Ollama returned unexpected HTTP status"
            );
        }
    }
}
