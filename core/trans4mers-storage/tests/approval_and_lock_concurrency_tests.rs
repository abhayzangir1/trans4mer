use chrono::Utc;
use std::sync::Arc;
use trans4mers_domain::approval::{Approval, ApprovalStatus};
use trans4mers_domain::ids::{AgentInstanceId, ApprovalId, ConversationId, ExecutionId};
use trans4mers_domain::tool::{Capability, RiskLevel};
use trans4mers_storage::db_handle::DbHandle;
use trans4mers_storage::migration_runner::MigrationRunner;
use trans4mers_storage::repos::approval_repo;
use trans4mers_storage::repos::lock_repo::LockRepo;

fn create_test_db() -> (Arc<DbHandle>, std::path::PathBuf) {
    let unique = format!("t4m_lock_test_{}.sqlite", uuid::Uuid::new_v4());
    let db_path = std::env::temp_dir().join(unique);
    let db = Arc::new(DbHandle::open(&db_path).unwrap());

    db.with_exclusive_conn(|conn| MigrationRunner::run_project_migrations(conn, 384))
        .unwrap();

    (db, db_path)
}

#[test]
fn test_lock_acquire_contention_and_release() {
    let (db, db_path) = create_test_db();
    let repo = LockRepo::new(db.clone());
    let key = "file:test_project:src/main.rs";

    // 1. Worker 1 acquires lock
    let acq1 = repo.try_acquire(key, "worker-1", 60).unwrap();
    assert!(acq1, "Worker 1 should acquire fresh lock");

    // 2. Worker 2 attempts acquisition -> must be blocked
    let acq2 = repo.try_acquire(key, "worker-2", 60).unwrap();
    assert!(!acq2, "Worker 2 should be rejected while Worker 1 holds lock");

    // 3. Verify holder is Worker 1
    assert_eq!(repo.holder(key).unwrap(), Some("worker-1".to_string()));

    // 4. Worker 1 renews lease
    let renewed = repo.renew(key, "worker-1", 120).unwrap();
    assert!(renewed, "Worker 1 renewal should succeed");

    // 5. Worker 2 cannot renew Worker 1's lock
    let renewed_fake = repo.renew(key, "worker-2", 120).unwrap();
    assert!(!renewed_fake, "Worker 2 cannot renew lock owned by Worker 1");

    // 6. Worker 1 releases lock
    repo.release(key, "worker-1").unwrap();

    // 7. Worker 2 can now acquire lock
    let acq3 = repo.try_acquire(key, "worker-2", 60).unwrap();
    assert!(acq3, "Worker 2 can acquire previously released lock");
    assert_eq!(repo.holder(key).unwrap(), Some("worker-2".to_string()));

    let _ = std::fs::remove_file(&db_path);
}

#[test]
fn test_lock_ttl_expiration_and_purge() {
    let (db, db_path) = create_test_db();
    let repo = LockRepo::new(db.clone());
    let key = "resource:task_slot_1";

    // Acquire with TTL of 1 second
    assert!(repo.try_acquire(key, "worker-ephemeral", 1).unwrap());

    // Sleep 1.2s to let TTL elapse
    std::thread::sleep(std::time::Duration::from_millis(1200));

    // Holder query should now return None since lock expired
    assert_eq!(repo.holder(key).unwrap(), None);

    // Another worker can now steal the expired lock
    assert!(repo.try_acquire(key, "worker-successor", 60).unwrap());
    assert_eq!(repo.holder(key).unwrap(), Some("worker-successor".to_string()));

    let _ = std::fs::remove_file(&db_path);
}

#[test]
fn test_approval_lifecycle_pending_to_approved() {
    let (db, db_path) = create_test_db();
    let approval_id = ApprovalId::new();
    let execution_id = ExecutionId::new();
    let agent_id = AgentInstanceId::new();
    let convo_id = ConversationId::new();

    let app = Approval {
        id: approval_id.clone(),
        execution_id: execution_id.clone(),
        agent_instance_id: agent_id,
        conversation_id: convo_id,
        capability: Capability::ShellExecute,
        tool_name: Some("shell.exec".to_string()),
        action_description: "Run cargo check".to_string(),
        arguments_summary: "cargo check".to_string(),
        arguments_hash: Some("sha256_canonical_12345".to_string()),
        risk_level: RiskLevel::Medium,
        status: ApprovalStatus::Pending,
        human_feedback: None,
        requested_at: Utc::now(),
        expires_at: Utc::now() + chrono::Duration::hours(1),
        resolved_at: None,
    };

    // 1. Insert approval
    db.with_write_tx(|conn| approval_repo::insert_approval(conn, &app)).unwrap();

    // 2. Query approval
    let fetched = db.with_read_conn(|conn| approval_repo::get_approval(conn, &approval_id)).unwrap();
    assert!(fetched.is_some());
    let fetched = fetched.unwrap();
    assert_eq!(fetched.status, ApprovalStatus::Pending);
    assert_eq!(fetched.tool_name, Some("shell.exec".to_string()));

    // 3. has_approved before resolution -> false
    let is_app = db.with_read_conn(|conn| {
        approval_repo::has_approved(conn, &execution_id, "shell.exec", "sha256_canonical_12345")
    }).unwrap();
    assert!(!is_app, "Must not be approved while Pending");

    // 4. Resolve approval -> Approved
    db.with_write_tx(|conn| {
        approval_repo::resolve_approval(conn, &approval_id, ApprovalStatus::Approved, Some("Allowed".to_string()))
    }).unwrap();

    // 5. has_approved after resolution -> true
    let is_app_now = db.with_read_conn(|conn| {
        approval_repo::has_approved(conn, &execution_id, "shell.exec", "sha256_canonical_12345")
    }).unwrap();
    assert!(is_app_now, "Must be approved after operator resolves Approved");

    // 6. has_approved with tampered arguments hash -> false (anti-TOCTOU)
    let is_app_tampered = db.with_read_conn(|conn| {
        approval_repo::has_approved(conn, &execution_id, "shell.exec", "tampered_hash_9999")
    }).unwrap();
    assert!(!is_app_tampered, "Tampered argument hash must return false");

    let _ = std::fs::remove_file(&db_path);
}

#[test]
fn test_approval_lifecycle_rejected() {
    let (db, db_path) = create_test_db();
    let approval_id = ApprovalId::new();
    let execution_id = ExecutionId::new();

    let app = Approval {
        id: approval_id.clone(),
        execution_id: execution_id.clone(),
        agent_instance_id: AgentInstanceId::new(),
        conversation_id: ConversationId::new(),
        capability: Capability::FilesystemWrite,
        tool_name: Some("fs.write".to_string()),
        action_description: "Overwriting config.json".to_string(),
        arguments_summary: "config.json".to_string(),
        arguments_hash: Some("sha256_config_hash".to_string()),
        risk_level: RiskLevel::High,
        status: ApprovalStatus::Pending,
        human_feedback: None,
        requested_at: Utc::now(),
        expires_at: Utc::now() + chrono::Duration::hours(1),
        resolved_at: None,
    };

    db.with_write_tx(|conn| approval_repo::insert_approval(conn, &app)).unwrap();

    // Reject with reason
    db.with_write_tx(|conn| {
        approval_repo::resolve_approval(conn, &approval_id, ApprovalStatus::Rejected, Some("Denied by security policy".to_string()))
    }).unwrap();

    let is_app = db.with_read_conn(|conn| {
        approval_repo::has_approved(conn, &execution_id, "fs.write", "sha256_config_hash")
    }).unwrap();
    assert!(!is_app, "Rejected approval must return false for has_approved");

    let _ = std::fs::remove_file(&db_path);
}
