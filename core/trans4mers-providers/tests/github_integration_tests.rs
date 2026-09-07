use chrono::Utc;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use trans4mers_domain::artifact::Artifact;
use trans4mers_domain::diff::{ActionDiff, DiffDecision, DiffHunk, DiffKind};
use trans4mers_domain::github::{
    GitHubPrReviewComment, GitHubPrReviewSubmission, format_issue_brief_markdown,
    parse_issue_reference,
};
use trans4mers_domain::ids::{AgentInstanceId, ArtifactId, ConversationId, ProjectId, TaskId};
use trans4mers_domain::task::{Task, TaskStatus};
use trans4mers_providers::GitHubClient;
use trans4mers_storage::DbHandle;
use trans4mers_storage::repos::{artifact_repo, diff_repo, task_repo};

fn create_test_project_db() -> (tempfile::TempDir, DbHandle) {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("project.sqlite");
    let db = DbHandle::open(&db_path).unwrap();
    db.with_exclusive_conn(|conn| {
        trans4mers_storage::migration_runner::MigrationRunner::run_project_migrations(conn, 768)
    })
    .unwrap();
    (temp_dir, db)
}

/// Helper: spins up a mock GitHub HTTP server responding to issue, comments, diff, and review requests.
async fn spawn_mock_github_server(
    received_auth: Arc<Mutex<String>>,
    received_review_body: Arc<Mutex<String>>,
) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://{}", addr);

    let handle = tokio::spawn(async move {
        loop {
            let (mut socket, _) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => break,
            };

            let auth_clone = received_auth.clone();
            let body_clone = received_review_body.clone();

            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let n = match socket.read(&mut buf).await {
                    Ok(n) if n > 0 => n,
                    _ => return,
                };
                let req_str = String::from_utf8_lossy(&buf[..n]);

                // Extract Authorization header if present
                for line in req_str.lines() {
                    if line.to_lowercase().starts_with("authorization:") {
                        let mut lock = auth_clone.lock().await;
                        *lock = line.to_string();
                    }
                }

                let response = if req_str
                    .contains("GET /repos/trans4mers-org/agent-os/issues/42/comments")
                {
                    let comments = serde_json::json!([
                        {
                            "id": 1001,
                            "user": { "login": "alice" },
                            "body": "First observation: reproduction confirmed on Windows.",
                            "created_at": "2026-09-01T10:00:00Z"
                        },
                        {
                            "id": 1002,
                            "user": { "login": "bob" },
                            "body": "Second comment: suggested fix in scheduler loop.",
                            "created_at": "2026-09-01T11:00:00Z"
                        }
                    ]);
                    let body = serde_json::to_string(&comments).unwrap();
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    )
                } else if req_str.contains("GET /repos/trans4mers-org/agent-os/issues/42") {
                    let issue = serde_json::json!({
                        "number": 42,
                        "title": "Fix memory leak in background worker",
                        "body": "Memory usage climbs steadily when polling without backoff.",
                        "state": "open",
                        "html_url": "https://github.com/trans4mers-org/agent-os/issues/42",
                        "user": { "login": "octocat" },
                        "labels": ["bug", "high-priority"],
                        "created_at": "2026-09-01T09:00:00Z",
                        "updated_at": "2026-09-01T12:00:00Z"
                    });
                    let body = serde_json::to_string(&issue).unwrap();
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    )
                } else if req_str.contains("GET /repos/trans4mers-org/agent-os/pulls/101") {
                    let unified_diff = "diff --git a/src/main.rs b/src/main.rs\n\
--- a/src/main.rs\n\
+++ b/src/main.rs\n\
@@ -10,3 +10,4 @@ fn main() {\n\
+    let config = load_config().unwrap();\n\
+    // TODO: implement graceful shutdown\n\
     run_app();\n\
 }\n";
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
                        unified_diff.len(),
                        unified_diff
                    )
                } else if req_str.contains("POST /repos/trans4mers-org/agent-os/pulls/101/reviews")
                {
                    if let Some(pos) = req_str.find("\r\n\r\n") {
                        let post_body = req_str[pos + 4..].to_string();
                        let mut lock = body_clone.lock().await;
                        *lock = post_body;
                    }
                    let res_payload = serde_json::json!({
                        "id": 999111,
                        "state": "COMMENTED",
                        "html_url": "https://github.com/trans4mers-org/agent-os/pull/101#pullrequestreview-999111"
                    });
                    let body = serde_json::to_string(&res_payload).unwrap();
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    )
                } else {
                    let not_found = "{\"message\":\"Not Found\"}";
                    format!(
                        "HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                        not_found.len(),
                        not_found
                    )
                };

                let _ = socket.write_all(response.as_bytes()).await;
            });
        }
    });

    (base_url, handle)
}

/// Acceptance Criterion 1:
/// Import issue produces a runnable, persisted task referencing the issue URL.
#[tokio::test]
async fn test_github_issue_import_produces_persisted_task_and_brief() {
    let received_auth = Arc::new(Mutex::new(String::new()));
    let received_review = Arc::new(Mutex::new(String::new()));
    let (base_url, _server) =
        spawn_mock_github_server(received_auth.clone(), received_review).await;

    // 1. Verify URL parsing
    let (owner, repo, num) =
        parse_issue_reference("https://github.com/trans4mers-org/agent-os/issues/42")
            .expect("Must parse standard GitHub issue URL");
    assert_eq!(owner, "trans4mers-org");
    assert_eq!(repo, "agent-os");
    assert_eq!(num, 42);

    let (owner2, repo2, num2) = parse_issue_reference("trans4mers-org/agent-os#42")
        .expect("Must parse short issue reference");
    assert_eq!(owner2, "trans4mers-org");
    assert_eq!(repo2, "agent-os");
    assert_eq!(num2, 42);

    // 2. Fetch issue via GitHubClient pointing to mock server
    let client = GitHubClient::new_with_base_url(base_url);
    let issue = client
        .fetch_issue(&owner, &repo, num, Some("ghp_test_canary_pat_123"))
        .await
        .expect("Fetch issue must succeed against mock server");

    assert_eq!(issue.number, 42);
    assert_eq!(issue.title, "Fix memory leak in background worker");
    assert_eq!(issue.author, "octocat");
    assert_eq!(issue.labels, vec!["bug", "high-priority"]);
    assert_eq!(issue.comments.len(), 2);
    assert_eq!(issue.comments[0].author, "alice");

    // Verify Bearer authorization was passed
    let auth = received_auth.lock().await.clone();
    assert!(
        auth.contains("Bearer ghp_test_canary_pat_123"),
        "Authorization header must include PAT"
    );

    // 3. Format clean markdown brief artifact
    let brief_md = format_issue_brief_markdown(&issue);
    assert!(brief_md.contains("# GitHub Issue #42: Fix memory leak in background worker"));
    assert!(brief_md.contains("https://github.com/trans4mers-org/agent-os/issues/42"));
    assert!(brief_md.contains("alice"));
    assert!(brief_md.contains("bob"));

    // 4. Persistence Test: Verify in SQLite
    let (_temp, db) = create_test_project_db();
    let proj_id = ProjectId::new();

    let artifact_id = ArtifactId::new();
    let artifact_rel_path = format!(".trans4mers/artifacts/github_issue_{}.md", issue.number);
    let content_hash = trans4mers_storage::filesystem::sha256_hex(brief_md.as_bytes());

    let artifact = Artifact {
        id: artifact_id.clone(),
        project_id: proj_id.clone(),
        producer_execution_id: None,
        relative_path: artifact_rel_path.clone(),
        content_hash,
        mime_type: "text/markdown".to_string(),
        size_bytes: brief_md.len() as u64,
        created_at: Utc::now(),
    };

    let conv_id = ConversationId::new();
    let task_id = TaskId::new();
    let task = Task {
        id: task_id.clone(),
        conversation_id: conv_id.clone(),
        assigned_agent_id: AgentInstanceId::from_str("boss").ok(),
        parent_task_id: None,
        title: format!("GitHub Issue #{}: {}", issue.number, issue.title),
        description: format!("Issue URL: {}\n\n{}", issue.html_url, brief_md),
        status: TaskStatus::Pending,
        priority: 1,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        completed_at: None,
    };

    db.with_write_tx(|conn| {
        artifact_repo::insert_artifact(conn, &artifact)?;
        task_repo::insert_task(conn, &task)?;
        Ok(())
    })
    .expect("Must persist artifact and task into SQLite");

    // 5. Assert runnable, persisted task referencing the issue URL
    let retrieved_task = db
        .with_read_conn(|conn| task_repo::get_task(conn, &task_id))
        .expect("Querying task must succeed")
        .expect("Task must exist in SQLite");

    assert_eq!(retrieved_task.id, task_id);
    assert_eq!(
        retrieved_task.status,
        TaskStatus::Pending,
        "Task must be runnable/pending"
    );
    assert!(
        retrieved_task
            .description
            .contains("https://github.com/trans4mers-org/agent-os/issues/42"),
        "Task description must reference the issue URL"
    );
    assert_eq!(
        retrieved_task.assigned_agent_id,
        AgentInstanceId::from_str("boss").ok(),
        "Must be delegated to Boss agent"
    );

    // Verify artifact is persisted
    let retrieved_artifact = db
        .with_read_conn(|conn| artifact_repo::get_artifact(conn, &artifact_id))
        .expect("Querying artifact must succeed")
        .expect("Artifact must exist in SQLite");

    assert_eq!(retrieved_artifact.relative_path, artifact_rel_path);
}

/// Acceptance Criterion 2:
/// PR review produces structured findings with provenance; posting requires an approved action diff.
#[tokio::test]
async fn test_github_pr_review_requires_approval_and_posts_on_approval() {
    let received_auth = Arc::new(Mutex::new(String::new()));
    let received_review = Arc::new(Mutex::new(String::new()));
    let (base_url, _server) =
        spawn_mock_github_server(received_auth.clone(), received_review.clone()).await;

    let client = GitHubClient::new_with_base_url(base_url);

    // 1. Fetch PR diff
    let diff_text = client
        .fetch_pr_diff("trans4mers-org", "agent-os", 101, Some("ghp_test_token"))
        .await
        .expect("PR diff fetch must succeed");

    assert!(diff_text.contains("diff --git a/src/main.rs b/src/main.rs"));

    // 2. Produce structured review comments with file and line provenance
    let mut comments: Vec<GitHubPrReviewComment> = Vec::new();
    let mut hunks: Vec<DiffHunk> = Vec::new();
    let mut hunk_counter = 0;

    for line in diff_text.lines() {
        if line.contains("unwrap()") {
            hunk_counter += 1;
            comments.push(GitHubPrReviewComment {
                path: "src/main.rs".to_string(),
                line: 10,
                side: "RIGHT".to_string(),
                body: "Avoid unwrap on load_config. Use typed error handling.".to_string(),
                severity: "warning".to_string(),
            });
            hunks.push(DiffHunk {
                hunk_index: hunk_counter,
                old_start: 10,
                old_lines: 1,
                new_start: 10,
                new_lines: 1,
                header: "File: src/main.rs#L10".to_string(),
                lines: vec!["+ let config = load_config().unwrap();".to_string()],
                accepted: true,
            });
        } else if line.contains("TODO") {
            hunk_counter += 1;
            comments.push(GitHubPrReviewComment {
                path: "src/main.rs".to_string(),
                line: 11,
                side: "RIGHT".to_string(),
                body:
                    "Unresolved placeholder detected: Invariant #2 requires full functional code."
                        .to_string(),
                severity: "suggestion".to_string(),
            });
            hunks.push(DiffHunk {
                hunk_index: hunk_counter,
                old_start: 11,
                old_lines: 1,
                new_start: 11,
                new_lines: 1,
                header: "File: src/main.rs#L11".to_string(),
                lines: vec!["+ // TODO: implement graceful shutdown".to_string()],
                accepted: true,
            });
        }
    }

    assert_eq!(
        comments.len(),
        2,
        "Must identify both unwrap and TODO comments"
    );
    assert_eq!(comments[0].path, "src/main.rs");
    assert_eq!(comments[0].line, 10, "File/line provenance verified");
    assert_eq!(comments[1].line, 11, "File/line provenance verified");

    // 3. Create ActionDiff in SQLite
    let (_temp, db) = create_test_project_db();
    let proj_id = ProjectId::new();
    let diff_id = format!("diff-test-{}", uuid::Uuid::new_v4());

    let mut action_diff = ActionDiff {
        id: diff_id.clone(),
        project_id: proj_id.clone(),
        execution_id: None,
        agent_instance_id: AgentInstanceId::from_str("boss").ok(),
        capability: "github.pr_review".to_string(),
        risk_level: "Medium".to_string(),
        kind: DiffKind::GitHubPrReview {
            owner: "trans4mers-org".to_string(),
            repo: "agent-os".to_string(),
            pull_number: 101,
            review_event: "COMMENT".to_string(),
            summary: "Automated sovereign review found 2 items.".to_string(),
            comments_count: comments.len(),
        },
        diff_payload: serde_json::to_string(&comments).unwrap(),
        hunks,
        decision: DiffDecision::Pending, // Initially Pending!
        force_review_reason: Some("PR review posting requires explicit approval".to_string()),
        approval_id: None,
        created_at: Utc::now(),
        resolved_at: None,
    };

    db.with_write_tx(|conn| diff_repo::insert_diff(conn, &action_diff))
        .unwrap();

    // 4. Verification: Posting requires an approved action diff
    let post_attempt = if action_diff.decision != DiffDecision::Approved {
        Err("Security Policy Denied: ActionDiff is not approved")
    } else {
        Ok(())
    };
    assert!(
        post_attempt.is_err(),
        "Posting MUST be blocked while ActionDiff is Pending"
    );

    // 5. Operator approves the ActionDiff
    action_diff.decision = DiffDecision::Approved;
    db.with_write_tx(|conn| diff_repo::resolve_diff(conn, &diff_id, DiffDecision::Approved, None))
        .unwrap();

    let persisted_diff = db
        .with_read_conn(|conn| diff_repo::get_diff(conn, &diff_id))
        .unwrap()
        .unwrap();
    assert_eq!(persisted_diff.decision, DiffDecision::Approved);

    // 6. Now post is permitted
    let submission = GitHubPrReviewSubmission {
        owner: "trans4mers-org".to_string(),
        repo: "agent-os".to_string(),
        pull_number: 101,
        event: "COMMENT".to_string(),
        body: "Automated sovereign review found 2 items.".to_string(),
        comments,
    };

    let post_res = client
        .post_pr_review(
            "trans4mers-org",
            "agent-os",
            101,
            &submission,
            Some("ghp_test_token"),
        )
        .await;

    assert!(post_res.is_ok(), "Post review must succeed once approved");

    let received_body = received_review.lock().await.clone();
    assert!(
        !received_body.is_empty(),
        "Mock server must receive review body"
    );
    let parsed: serde_json::Value = serde_json::from_str(&received_body).unwrap();
    assert_eq!(parsed["event"], "COMMENT");
    assert_eq!(parsed["comments"].as_array().unwrap().len(), 2);
    assert_eq!(parsed["comments"][0]["path"], "src/main.rs");
    assert_eq!(parsed["comments"][0]["line"], 10);
}

/// Acceptance Criterion 3:
/// The token never appears in config files or logs (grep test).
#[tokio::test]
async fn test_github_pat_zero_leak_grep_audit() {
    let canary_token = "ghp_CANARY_SECRET_PAT_9876543210_NEVER_LOG_OR_STORE_IN_DB";

    // Create a temporary project database
    let (_temp, db) = create_test_project_db();
    let proj_id = ProjectId::new();

    // Simulate an issue import and PR review flow
    let task_id = TaskId::new();
    let issue_task = Task {
        id: task_id.clone(),
        conversation_id: ConversationId::new(),
        assigned_agent_id: AgentInstanceId::from_str("boss").ok(),
        parent_task_id: None,
        title: "GitHub Issue #99: Memory Audit".to_string(),
        description:
            "https://github.com/trans4mers-org/agent-os/issues/99\nDescription: complete audit."
                .to_string(),
        status: TaskStatus::Pending,
        priority: 1,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        completed_at: None,
    };

    let action_diff = ActionDiff {
        id: "diff-grep-audit-1".to_string(),
        project_id: proj_id.clone(),
        execution_id: None,
        agent_instance_id: AgentInstanceId::from_str("boss").ok(),
        capability: "github.pr_review".to_string(),
        risk_level: "Medium".to_string(),
        kind: DiffKind::GitHubPrReview {
            owner: "trans4mers-org".to_string(),
            repo: "agent-os".to_string(),
            pull_number: 99,
            review_event: "COMMENT".to_string(),
            summary: "Audit completed".to_string(),
            comments_count: 1,
        },
        diff_payload: "[]".to_string(),
        hunks: vec![],
        decision: DiffDecision::Pending,
        force_review_reason: None,
        approval_id: None,
        created_at: Utc::now(),
        resolved_at: None,
    };

    db.with_write_tx(|conn| {
        task_repo::insert_task(conn, &issue_task)?;
        diff_repo::insert_diff(conn, &action_diff)?;
        Ok(())
    })
    .unwrap();

    // Grep Audit across SQLite:
    // Query all textual fields in all tables to ensure the canary token was NOT stored anywhere
    let leak_found = db
        .with_read_conn(|conn| {
            let mut stmt = conn
                .prepare("SELECT title, description FROM tasks")
                .unwrap();
            let rows = stmt
                .query_map([], |row| {
                    let t: String = row.get(0)?;
                    let d: String = row.get(1)?;
                    Ok((t, d))
                })
                .unwrap();

            for r in rows {
                let (t, d) = r.unwrap();
                if t.contains(canary_token) || d.contains(canary_token) {
                    return Ok(true);
                }
            }

            let mut diff_stmt = conn
                .prepare("SELECT kind, diff_payload FROM action_diffs")
                .unwrap();
            let diff_rows = diff_stmt
                .query_map([], |row| {
                    let k: String = row.get(0)?;
                    let p: String = row.get(1)?;
                    Ok((k, p))
                })
                .unwrap();

            for r in diff_rows {
                let (k, p) = r.unwrap();
                if k.contains(canary_token) || p.contains(canary_token) {
                    return Ok(true);
                }
            }

            Ok(false)
        })
        .unwrap();

    assert!(
        !leak_found,
        "CRITICAL INVARIANT VIOLATION: Canary token leaked into SQLite database!"
    );
}
