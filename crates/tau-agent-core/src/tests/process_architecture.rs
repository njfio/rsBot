use std::time::Duration;

use crate::{
    ProcessLifecycleState, ProcessManager, ProcessManagerError, ProcessRuntimeProfile,
    ProcessSpawnSpec, ProcessType,
};

#[test]
fn unit_process_runtime_profile_defaults_are_role_specific() {
    let channel = ProcessRuntimeProfile::for_type(ProcessType::Channel);
    assert_eq!(channel.process_type, ProcessType::Channel);
    assert_eq!(channel.max_turns, 8);
    assert_eq!(channel.max_context_messages, Some(256));
    assert!(channel.tool_allowlist.contains(&"branch".to_string()));

    let worker = ProcessRuntimeProfile::for_type(ProcessType::Worker);
    assert_eq!(worker.process_type, ProcessType::Worker);
    assert_eq!(worker.max_turns, 25);
    assert_eq!(worker.max_context_messages, Some(96));
    assert!(worker.tool_allowlist.contains(&"memory_search".to_string()));

    let cortex = ProcessRuntimeProfile::for_type(ProcessType::Cortex);
    assert_eq!(cortex.process_type, ProcessType::Cortex);
    assert_eq!(cortex.max_turns, 6);
    assert_eq!(cortex.max_context_messages, Some(192));
}

#[test]
fn regression_spec_2721_c04_channel_runtime_profile_exposes_worker_delegation_tool() {
    let channel = ProcessRuntimeProfile::for_type(ProcessType::Channel);
    assert!(
        channel.tool_allowlist.contains(&"worker".to_string()),
        "channel profile must include worker delegation capability"
    );
}

#[tokio::test]
async fn functional_process_manager_supervises_running_and_terminal_states() {
    let manager = ProcessManager::default();
    let worker_spec = ProcessSpawnSpec::new("worker-1", ProcessType::Worker)
        .with_parent_process_id("channel-1")
        .with_session_key("session-alpha");

    let handle = manager
        .spawn_supervised(worker_spec.clone(), |_spec| async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            Ok(())
        })
        .expect("worker spawn should succeed");

    tokio::time::sleep(Duration::from_millis(5)).await;
    let running = manager
        .snapshot("worker-1")
        .expect("running worker snapshot must exist");
    assert_eq!(running.process_type, ProcessType::Worker);
    assert!(
        running.state == ProcessLifecycleState::Pending
            || running.state == ProcessLifecycleState::Running
    );

    handle.await.expect("worker task join");
    let completed = manager
        .snapshot("worker-1")
        .expect("completed worker snapshot must exist");
    assert_eq!(completed.state, ProcessLifecycleState::Completed);
    assert!(completed.finished_unix_ms.is_some());

    let failing_spec = ProcessSpawnSpec::new("worker-2", ProcessType::Worker);
    let fail_handle = manager
        .spawn_supervised(failing_spec, |_spec| async move {
            Err("simulated failure".to_string())
        })
        .expect("failing worker spawn should still register");
    fail_handle.await.expect("failing worker join");
    let failed = manager
        .snapshot("worker-2")
        .expect("failed worker snapshot must exist");
    assert_eq!(failed.state, ProcessLifecycleState::Failed);
    assert_eq!(failed.error.as_deref(), Some("simulated failure"));
}

#[tokio::test]
async fn regression_process_manager_rejects_duplicate_process_ids() {
    let manager = ProcessManager::default();
    let spec = ProcessSpawnSpec::new("duplicate-worker", ProcessType::Worker);

    let handle = manager
        .spawn_supervised(spec.clone(), |_spec| async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            Ok(())
        })
        .expect("initial worker spawn should succeed");

    let duplicate = manager.spawn_supervised(spec, |_spec| async move { Ok(()) });
    assert!(matches!(
        duplicate,
        Err(ProcessManagerError::DuplicateProcessId(id)) if id == "duplicate-worker"
    ));

    handle.await.expect("initial worker join");
}
