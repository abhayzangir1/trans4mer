use git2::Repository;
use std::path::{Path, PathBuf};
use tracing::info;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::ids::AgentInstanceId;

pub struct GitWorkspace;

impl GitWorkspace {
    /// Ensure the workspace is a git repository. Auto-initialize using `git2` if not.
    /// Also ensures a strict `.gitignore` exists to prevent staging heavy dependency
    /// folders (`node_modules`, `target`) or isolated agent worktrees (`.trans4mers`).
    pub fn ensure_git_workspace(workspace_path: &Path) -> Result<(), Trans4mersError> {
        Self::ensure_gitignore(workspace_path)?;

        if !workspace_path.join(".git").exists() {
            tracing::info!(path = %workspace_path.display(), "Initializing git repository via git2");

            let mut opts = git2::RepositoryInitOptions::new();
            opts.initial_head("main");
            let repo = git2::Repository::init_opts(workspace_path, &opts)
                .map_err(|e| Trans4mersError::Internal(format!("git2 init failed: {}", e)))?;

            let mut index = repo
                .index()
                .map_err(|e| Trans4mersError::Internal(e.to_string()))?;
            index
                .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
                .map_err(|e| Trans4mersError::Internal(e.to_string()))?;
            index
                .write()
                .map_err(|e| Trans4mersError::Internal(e.to_string()))?;

            let oid = index
                .write_tree()
                .map_err(|e| Trans4mersError::Internal(e.to_string()))?;
            let sig = git2::Signature::now("Trans4mers", "trans4mers@local").unwrap();
            let tree = repo.find_tree(oid).unwrap();

            repo.commit(
                Some("HEAD"),
                &sig,
                &sig,
                "Initial commit - Trans4mers",
                &tree,
                &[],
            )
            .map_err(|e| Trans4mersError::Internal(format!("git2 commit failed: {}", e)))?;
        }
        Ok(())
    }

    /// Checks and updates `.gitignore` to guarantee `.trans4mers/` and build artifacts are never tracked.
    fn ensure_gitignore(workspace_path: &Path) -> Result<(), Trans4mersError> {
        let gitignore_path = workspace_path.join(".gitignore");
        if !gitignore_path.exists() {
            let default_ignore = "# Trans4mers isolated worktrees and ephemeral storage\n\
                .trans4mers/\n\
                *.log\n\
                \n\
                # Dependencies and build outputs\n\
                node_modules/\n\
                target/\n\
                dist/\n\
                build/\n\
                .next/\n\
                .turbo/\n\
                .cache/\n\
                \n\
                # Environment and secrets\n\
                .env\n\
                .env.*\n\
                !.env.example\n";
            std::fs::write(&gitignore_path, default_ignore).map_err(|e| {
                Trans4mersError::Internal(format!("Failed to create .gitignore: {}", e))
            })?;
        } else {
            let content = std::fs::read_to_string(&gitignore_path).unwrap_or_default();
            if !content.contains(".trans4mers") {
                let mut updated = content;
                if !updated.ends_with('\n') {
                    updated.push('\n');
                }
                updated.push_str("\n# Trans4mers agent worktree isolation\n.trans4mers/\n");
                let _ = std::fs::write(&gitignore_path, updated);
            }
        }
        Ok(())
    }

    /// Creates a new isolated git worktree for a specific agent using genuine git2 bindings.
    /// The agent can make destructive changes here without affecting the main project tree
    /// or other concurrent agents.
    pub fn create_agent_worktree(
        base_repo_path: &Path,
        agent_id: &AgentInstanceId,
        branch_name: &str,
    ) -> Result<PathBuf, Trans4mersError> {
        Self::ensure_gitignore(base_repo_path)?;

        let worktree_dir = base_repo_path.join(format!(".trans4mers/worktrees/{}", agent_id));

        // Ensure the base directory exists
        if let Some(parent) = worktree_dir.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                Trans4mersError::Internal(format!("Failed to create worktree dir: {}", e))
            })?;
        }

        info!(
            "Creating git worktree for agent {} at {:?}",
            agent_id, worktree_dir
        );

        let repo = Repository::open(base_repo_path).map_err(|e| {
            Trans4mersError::Internal(format!("Failed to open base git repo: {}", e))
        })?;

        // Resolve HEAD to create the branch from
        let head_commit = repo.head().and_then(|h| h.peel_to_commit()).map_err(|e| {
            Trans4mersError::Internal(format!("Failed to resolve HEAD commit: {}", e))
        })?;

        // Create the isolated branch
        let branch = repo.branch(branch_name, &head_commit, false).map_err(|e| {
            Trans4mersError::Internal(format!(
                "Failed to create isolated branch '{}': {}",
                branch_name, e
            ))
        })?;

        let branch_ref = branch.into_reference();

        // Configure the worktree creation to link to the new branch
        let mut opts = git2::WorktreeAddOptions::new();
        opts.reference(Some(&branch_ref));

        let worktree_name = format!("{}", agent_id);

        repo.worktree(&worktree_name, &worktree_dir, Some(&opts))
            .map_err(|e| {
                Trans4mersError::Internal(format!("Failed to instantiate git worktree: {}", e))
            })?;

        Ok(worktree_dir)
    }

    /// Removes an agent's git worktree once their task is completed.
    /// Prunes from git registry and thoroughly wipes the worktree directory from disk.
    pub fn cleanup_agent_worktree(
        base_repo_path: &Path,
        worktree_name: &str,
    ) -> Result<(), Trans4mersError> {
        info!("Cleaning up git worktree {}", worktree_name);

        let repo = Repository::open(base_repo_path).map_err(|e| {
            Trans4mersError::Internal(format!("Failed to open base git repo: {}", e))
        })?;

        if let Ok(worktree) = repo.find_worktree(worktree_name) {
            let mut prune_opts = git2::WorktreePruneOptions::new();
            prune_opts.valid(true).working_tree(true);

            let _ = worktree.prune(Some(&mut prune_opts));
        }

        // Thoroughly wipe directory from disk to avoid orphaned clutter
        let worktree_dir = base_repo_path
            .join(".trans4mers/worktrees")
            .join(worktree_name);
        if worktree_dir.exists() {
            let _ = std::fs::remove_dir_all(&worktree_dir);
        }

        Ok(())
    }

    /// Stages all modified/untracked files in the agent's worktree and creates a commit if changes exist.
    pub fn commit_worktree_changes(
        worktree_dir: &Path,
        commit_message: &str,
    ) -> Result<bool, Trans4mersError> {
        info!("Committing worktree changes at {:?}", worktree_dir);

        let repo = Repository::open(worktree_dir).map_err(|e| {
            Trans4mersError::Internal(format!("Failed to open worktree repo: {}", e))
        })?;

        let mut index = repo.index().map_err(|e| {
            Trans4mersError::Internal(format!("Failed to get worktree index: {}", e))
        })?;

        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .map_err(|e| {
                Trans4mersError::Internal(format!("Failed to stage worktree files: {}", e))
            })?;

        index.write().map_err(|e| {
            Trans4mersError::Internal(format!("Failed to write worktree index: {}", e))
        })?;

        let oid = index.write_tree().map_err(|e| {
            Trans4mersError::Internal(format!("Failed to write tree from index: {}", e))
        })?;

        let tree = repo.find_tree(oid).map_err(|e| {
            Trans4mersError::Internal(format!("Failed to find tree for oid {}: {}", oid, e))
        })?;

        let head = repo.head().ok();
        let head_commit = head.as_ref().and_then(|h| h.peel_to_commit().ok());

        // Check if there are actual diffs between the previous commit and the staged tree
        if let Some(ref parent) = head_commit
            && parent.tree().map(|t| t.id() == oid).unwrap_or(false)
        {
            // No changes were made to the worktree
            return Ok(false);
        }

        let sig = git2::Signature::now("Trans4mers Agent", "agent@trans4mers.local")
            .map_err(|e| Trans4mersError::Internal(format!("Failed to create signature: {}", e)))?;

        let parents = match &head_commit {
            Some(c) => vec![c],
            None => vec![],
        };

        repo.commit(Some("HEAD"), &sig, &sig, commit_message, &tree, &parents)
            .map_err(|e| {
                Trans4mersError::Internal(format!("Failed to commit worktree changes: {}", e))
            })?;

        Ok(true)
    }

    /// Merge an agent's worktree back into the main branch using `git2`.
    pub fn merge_agent_branch(
        workspace_path: &Path,
        branch_name: &str,
    ) -> Result<(), Trans4mersError> {
        info!(
            "Merging agent branch {} back into main workspace at {:?}",
            branch_name, workspace_path
        );

        let repo = Repository::open(workspace_path)
            .map_err(|e| Trans4mersError::Internal(e.to_string()))?;

        let branch = repo
            .find_branch(branch_name, git2::BranchType::Local)
            .map_err(|e| Trans4mersError::Internal(format!("Branch not found: {}", e)))?;

        let annotated = repo
            .reference_to_annotated_commit(branch.get())
            .map_err(|e| Trans4mersError::Internal(e.to_string()))?;

        // Perform merge analysis
        let (analysis, _) = repo
            .merge_analysis(&[&annotated])
            .map_err(|e| Trans4mersError::Internal(e.to_string()))?;

        if analysis.is_up_to_date() {
            return Ok(());
        }

        if analysis.is_fast_forward() {
            let mut reference = repo
                .head()
                .map_err(|e| Trans4mersError::Internal(format!("Failed to resolve HEAD: {}", e)))?;
            reference
                .set_target(annotated.id(), "Fast-Forward Merge")
                .map_err(|e| {
                    Trans4mersError::Internal(format!("Failed to set reference target: {}", e))
                })?;
            repo.set_head(reference.name().unwrap_or("HEAD"))
                .map_err(|e| Trans4mersError::Internal(format!("Failed to set head: {}", e)))?;
            repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))
                .map_err(|e| {
                    Trans4mersError::Internal(format!("Failed to checkout head: {}", e))
                })?;
        } else if analysis.is_normal() {
            let head_commit = repo.head().and_then(|h| h.peel_to_commit()).map_err(|e| {
                Trans4mersError::Internal(format!("Failed to peel HEAD to commit: {}", e))
            })?;
            let opts = git2::MergeOptions::new();
            let merge_commit = repo
                .find_commit(annotated.id())
                .map_err(|e| Trans4mersError::Internal(format!("Failed to find commit: {}", e)))?;
            let mut index = repo
                .merge_commits(&head_commit, &merge_commit, Some(&opts))
                .map_err(|e| {
                    Trans4mersError::Internal(format!("Failed to merge commits: {}", e))
                })?;

            if index.has_conflicts() {
                return Err(Trans4mersError::Internal("Git merge failed: merge conflicts detected between sub-agent worktree and main branch".to_string()));
            }

            let oid = index.write_tree_to(&repo).map_err(|e| {
                Trans4mersError::Internal(format!("Failed to write merge tree: {}", e))
            })?;
            let sig = git2::Signature::now("Trans4mers", "trans4mers@local").map_err(|e| {
                Trans4mersError::Internal(format!("Failed to create signature: {}", e))
            })?;
            let tree = repo
                .find_tree(oid)
                .map_err(|e| Trans4mersError::Internal(format!("Failed to find tree: {}", e)))?;

            repo.commit(
                Some("HEAD"),
                &sig,
                &sig,
                &format!("Merge branch '{}'", branch_name),
                &tree,
                &[&head_commit, &merge_commit],
            )
            .map_err(|e| {
                Trans4mersError::Internal(format!("Failed to create merge commit: {}", e))
            })?;

            repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))
                .map_err(|e| {
                    Trans4mersError::Internal(format!("Failed to checkout after merge: {}", e))
                })?;
        } else {
            return Err(Trans4mersError::Internal(
                "Git merge failed: unknown analysis type".to_string(),
            ));
        }

        Ok(())
    }
}
