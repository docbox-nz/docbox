use docbox_database::models::tasks::{Task, TaskStatus};
use uuid::Uuid;

use crate::common::{database::test_tenant_db, make_test_document_box};

mod common;

/// Tests a task can be created
#[tokio::test]
async fn test_task_create() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, _root) = make_test_document_box(&db, "test", None).await;

    let task = Task::create(&db, document_box.scope.clone()).await.unwrap();
    assert_eq!(task.status, TaskStatus::Pending);
    assert_eq!(task.output_data, None);
    assert_eq!(task.completed_at, None);
}

/// Tests that a task should not be able to be created in a document box that
/// does not exist
#[tokio::test]
async fn test_task_create_with_unknown_document_box() {
    let (db, _db_container) = test_tenant_db().await;
    let err = Task::create(&db, "unknown".to_string()).await.unwrap_err();

    // Shouldn't be able to create a task where the document box doesn't match
    assert!(
        err.into_database_error()
            .unwrap()
            .is_foreign_key_violation()
    );
}

/// Tests that deleting a document box will cascade and delete tasks within
#[tokio::test]
async fn test_task_cascade_delete() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, root) = make_test_document_box(&db, "test", None).await;

    // Must delete the root folder to delete the document box
    root.delete(&db).await.unwrap();

    let task = Task::create(&db, document_box.scope.clone()).await.unwrap();
    document_box.delete(&db).await.unwrap();

    // Task should not exist after document box deletion
    let task = Task::find(&db, task.id, &document_box.scope).await.unwrap();
    assert!(task.is_none());
}

/// Tests that a task can be found by ID
#[tokio::test]
async fn test_find_task_by_id() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, _root) = make_test_document_box(&db, "test", None).await;

    let task = Task::create(&db, document_box.scope.clone()).await.unwrap();
    let found_task = Task::find(&db, task.id, &document_box.scope)
        .await
        .unwrap()
        .expect("task should exist");

    assert_eq!(task, found_task);

    let found_task = Task::find(&db, task.id, "unknown").await.unwrap();
    assert!(found_task.is_none());

    let found_task = Task::find(&db, Uuid::nil(), &document_box.scope)
        .await
        .unwrap();
    assert!(found_task.is_none());
}

/// Tests that a task can be completed
#[tokio::test]
async fn test_complete_task() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, _root) = make_test_document_box(&db, "test", None).await;

    let mut task = Task::create(&db, document_box.scope.clone()).await.unwrap();
    assert_eq!(task.status, TaskStatus::Pending);
    assert_eq!(task.output_data, None);
    assert_eq!(task.completed_at, None);

    task.complete_task(&db, TaskStatus::Failed, None)
        .await
        .unwrap();

    assert_eq!(task.status, TaskStatus::Failed);
    assert_eq!(task.output_data, None);
    assert!(task.completed_at.is_some());

    let test_output_value = serde_json::json!({
        "test": true
    });

    task.complete_task(&db, TaskStatus::Failed, Some(test_output_value.clone()))
        .await
        .unwrap();

    assert_eq!(task.status, TaskStatus::Failed);
    assert_eq!(task.output_data, Some(test_output_value.clone()));
    assert!(task.completed_at.is_some());

    let found_task = Task::find(&db, task.id, &document_box.scope)
        .await
        .unwrap()
        .expect("task should exist");
    assert_eq!(task, found_task);

    task.complete_task(&db, TaskStatus::Completed, Some(test_output_value.clone()))
        .await
        .unwrap();

    assert_eq!(task.status, TaskStatus::Completed);
    assert_eq!(task.output_data, Some(test_output_value.clone()));
    assert!(task.completed_at.is_some());

    let found_task = Task::find(&db, task.id, &document_box.scope)
        .await
        .unwrap()
        .expect("task should exist");
    assert_eq!(task, found_task);
}
