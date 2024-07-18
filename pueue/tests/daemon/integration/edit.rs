use std::path::PathBuf;

use anyhow::{bail, Result};
use test_log::test;

use pueue_lib::network::message::*;
use pueue_lib::settings::Shared;
use pueue_lib::state::GroupStatus;
use pueue_lib::task::*;

use crate::helper::*;

async fn create_edited_task(shared: &Shared) -> Result<EditResponseMessage> {
    // Add a task
    assert_success(add_task(shared, "ls").await?);

    // The task should now be queued
    assert_eq!(get_task_status(shared, 0).await?, TaskStatus::Queued);

    // Send a request to edit that task
    let response = send_message(shared, Message::EditRequest(0)).await?;
    if let Message::EditResponse(payload) = response {
        Ok(payload)
    } else {
        bail!("Didn't receive EditResponse after requesting edit.")
    }
}

/// Test if adding a normal task works as intended.
#[test(tokio::test(flavor = "multi_thread", worker_threads = 2))]
async fn test_edit_flow() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Pause the daemon. That way the command won't be started.
    pause_tasks(shared, TaskSelection::All).await?;
    wait_for_group_status(shared, PUEUE_DEFAULT_GROUP, GroupStatus::Paused).await?;

    let response = create_edited_task(shared).await?;
    assert_eq!(response.task_id, 0);
    assert_eq!(response.command, "ls");
    assert_eq!(response.path, daemon.tempdir.path());
    assert_eq!(response.priority, 0);

    // Task should be locked, after the request for editing succeeded.
    assert!(
        matches!(get_task_status(shared, 0).await?, TaskStatus::Locked { .. }),
        "Expected the task to be locked after first request."
    );

    // You cannot start a locked task. It should still be locked afterwards.
    start_tasks(shared, TaskSelection::TaskIds(vec![0])).await?;
    assert!(
        matches!(get_task_status(shared, 0).await?, TaskStatus::Locked { .. },),
        "Expected the task to still be locked."
    );

    // Send the final message of the protocol and actually change the task.
    let response = send_message(
        shared,
        EditMessage {
            task_id: 0,
            command: Some("ls -ahl".into()),
            path: Some("/tmp".into()),
            label: Some("test".to_string()),
            delete_label: false,
            priority: Some(99),
        },
    )
    .await?;
    assert_success(response);

    // Make sure the task has been changed and enqueued.
    let task = get_task(shared, 0).await?;
    assert_eq!(task.command, "ls -ahl");
    assert_eq!(task.path, PathBuf::from("/tmp"));
    assert_eq!(task.label, Some("test".to_string()));
    assert_eq!(task.status, TaskStatus::Queued);
    assert_eq!(task.priority, 99);

    Ok(())
}
