use chrono::Utc;
use rusqlite::Connection;
use tracing::info;
use trans4mers_domain::config::DiffReviewConfig;
use trans4mers_domain::diff::{ActionDiff, DiffDecision, DiffHunk, DiffKind};
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::ids::{AgentInstanceId, ApprovalId, ExecutionId, ProjectId};
use trans4mers_storage::repos::diff_repo;
use uuid::Uuid;

pub struct DiffReviewer;

impl DiffReviewer {
    pub fn compute_file_hunks(old_text: &str, new_text: &str) -> (Vec<DiffHunk>, usize, usize) {
        let old_lines: Vec<&str> = old_text.lines().collect();
        let new_lines: Vec<&str> = new_text.lines().collect();

        let n = old_lines.len();
        let m = new_lines.len();
        let mut dp = vec![vec![0; m + 1]; n + 1];

        for i in 1..=n {
            for j in 1..=m {
                if old_lines[i - 1] == new_lines[j - 1] {
                    dp[i][j] = dp[i - 1][j - 1] + 1;
                } else {
                    dp[i][j] = std::cmp::max(dp[i - 1][j], dp[i][j - 1]);
                }
            }
        }

        let mut lines = Vec::new();
        let mut additions = 0;
        let mut deletions = 0;

        let mut i = n;
        let mut j = m;

        while i > 0 && j > 0 {
            if old_lines[i - 1] == new_lines[j - 1] {
                lines.push(format!(" {}", old_lines[i - 1]));
                i -= 1;
                j -= 1;
            } else if dp[i - 1][j] >= dp[i][j - 1] {
                lines.push(format!("-{}", old_lines[i - 1]));
                deletions += 1;
                i -= 1;
            } else {
                lines.push(format!("+{}", new_lines[j - 1]));
                additions += 1;
                j -= 1;
            }
        }

        while i > 0 {
            lines.push(format!("-{}", old_lines[i - 1]));
            deletions += 1;
            i -= 1;
        }

        while j > 0 {
            lines.push(format!("+{}", new_lines[j - 1]));
            additions += 1;
            j -= 1;
        }

        lines.reverse();

        let mut hunks = Vec::new();
        if !lines.is_empty() {
            hunks.push(DiffHunk {
                hunk_index: 0,
                old_start: 1,
                old_lines: n,
                new_start: 1,
                new_lines: m,
                header: format!("@@ -1,{} +1,{} @@", n, m),
                lines,
                accepted: true,
            });
        }

        (hunks, additions, deletions)
    }

    /// Scans text for sensitive patterns (passwords, api keys, secret keys)
    pub fn scan_secrets(text: &str, patterns: &[String]) -> Option<String> {
        let lower = text.to_lowercase();
        for pattern in patterns {
            let pat_lower = pattern.to_lowercase();
            if lower.contains(&pat_lower) || text.contains(pattern) {
                return Some(format!("Contains sensitive secret pattern '{}'", pattern));
            }
        }
        None
    }

    /// Proposes an ActionDiff, evaluates auto-approve rules and secret overrides,
    /// and persists it to the database.
    #[allow(clippy::too_many_arguments)]
    pub fn propose(
        conn: &Connection,
        project_id: ProjectId,
        execution_id: Option<ExecutionId>,
        agent_instance_id: Option<AgentInstanceId>,
        capability: String,
        risk_level: String,
        kind: DiffKind,
        diff_payload: String,
        hunks: Vec<DiffHunk>,
        approval_id: Option<ApprovalId>,
        config: &DiffReviewConfig,
    ) -> Result<ActionDiff, Trans4mersError> {
        let diff_id = Uuid::new_v4().to_string();
        let force_review_reason =
            Self::scan_secrets(&diff_payload, &config.force_review_secret_patterns);

        let decision = if force_review_reason.is_some() {
            info!(
                "Diff {} force-review triggered: sensitive pattern detected",
                diff_id
            );
            DiffDecision::Pending
        } else {
            match &kind {
                DiffKind::FilePatch {
                    additions,
                    deletions,
                    ..
                } => {
                    if config.auto_approve_small_files && (additions + deletions) <= 10 {
                        DiffDecision::AutoApproved
                    } else {
                        DiffDecision::Pending
                    }
                }
                DiffKind::CommandExec { .. } => {
                    if config.auto_approve_clean_commands {
                        DiffDecision::AutoApproved
                    } else {
                        DiffDecision::Pending
                    }
                }
                DiffKind::MemoryMutation { importance, .. } => {
                    if *importance <= 0.5 {
                        DiffDecision::AutoApproved
                    } else {
                        DiffDecision::Pending
                    }
                }
                DiffKind::BrowserAction { verb, .. } => {
                    if verb == "navigate" || verb == "screenshot" || verb == "extract" {
                        DiffDecision::AutoApproved
                    } else {
                        DiffDecision::Pending
                    }
                }
                DiffKind::SkillProposal { .. } => {
                    // Skills always require human review
                    DiffDecision::Pending
                }
                DiffKind::ConfigChange { .. } => DiffDecision::Pending,
                DiffKind::GitHubPrReview { .. } => {
                    // Invariant: Nothing is posted to GitHub without explicit human approval
                    DiffDecision::Pending
                }
            }
        };

        let diff = ActionDiff {
            id: diff_id,
            project_id,
            execution_id,
            agent_instance_id,
            capability,
            risk_level,
            kind,
            diff_payload,
            hunks,
            decision,
            force_review_reason,
            approval_id,
            created_at: Utc::now(),
            resolved_at: if decision == DiffDecision::AutoApproved {
                Some(Utc::now())
            } else {
                None
            },
        };

        diff_repo::insert_diff(conn, &diff)?;
        Ok(diff)
    }
}
