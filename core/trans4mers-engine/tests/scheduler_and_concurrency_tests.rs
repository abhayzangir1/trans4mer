use std::sync::Arc;
use tokio::sync::Semaphore;
use trans4mers_domain::ids::{ExecutionId, ProjectId};
use trans4mers_engine::scheduler::Scheduler;

#[tokio::test]
async fn test_scheduler_permit_initialization_and_queue() {
    let (scheduler, mut new_rx, mut wake_rx) = Scheduler::new(4);
    let project_id = ProjectId::new();
    let exec_1 = ExecutionId::new();
    let exec_2 = ExecutionId::new();

    // 1. Queue executions
    scheduler.queue(exec_1.clone(), project_id.clone());
    scheduler.queue(exec_2.clone(), project_id.clone());

    // 2. Verify items received on new_rx channel
    let entry1 = new_rx.recv().await.unwrap();
    assert_eq!(entry1.execution_id, exec_1);
    assert_eq!(entry1.project_id, project_id);

    let entry2 = new_rx.recv().await.unwrap();
    assert_eq!(entry2.execution_id, exec_2);

    // 3. Test wake request
    scheduler.wake(exec_1.clone(), true);
    let wake = wake_rx.recv().await.unwrap();
    assert_eq!(wake.execution_id, exec_1);
    assert!(wake.priority_boost);
}

#[tokio::test]
async fn test_scheduler_slot_release_idempotent() {
    let (scheduler, _, _) = Scheduler::new(8);
    let proj_id = ProjectId::new();

    // Release slot safely without underflow
    scheduler.release_project_slot(&proj_id);
    scheduler.release_project_slot(&proj_id);
}

#[tokio::test]
async fn test_scheduler_permit_semaphore_concurrency() {
    let semaphore = Arc::new(Semaphore::new(2));

    // Acquire permit 1
    let permit1 = semaphore.clone().try_acquire_owned();
    assert!(permit1.is_ok(), "First permit must succeed");

    // Acquire permit 2
    let permit2 = semaphore.clone().try_acquire_owned();
    assert!(permit2.is_ok(), "Second permit must succeed");

    // Acquire permit 3 -> saturated, must fail
    let permit3 = semaphore.clone().try_acquire_owned();
    assert!(permit3.is_err(), "Third permit must fail when capacity is 2");

    // Drop permit 1
    drop(permit1);

    // Now permit 3 can be acquired
    let permit3_retry = semaphore.clone().try_acquire_owned();
    assert!(permit3_retry.is_ok(), "Permit must succeed after slot is freed");
}
