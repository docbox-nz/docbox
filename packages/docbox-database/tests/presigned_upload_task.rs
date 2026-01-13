use crate::common::{database::test_tenant_db, make_test_document_box};
use chrono::{Days, Utc};
use docbox_database::models::presigned_upload_task::{
    CreatePresignedUploadTask, PresignedTaskStatus, PresignedUploadTask,
};
use sqlx::types::Json;
use uuid::Uuid;

mod common;

/// Tests a presigned upload task can be deleted
#[tokio::test]
async fn test_presigned_upload_task_create() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, root) = make_test_document_box(&db, "test", None).await;

    let processing_config = serde_json::json!({
        "test": true
    });

    let task = PresignedUploadTask::create(
        &db,
        CreatePresignedUploadTask {
            name: "test".to_string(),
            mime: "text/plain".to_string(),
            document_box: document_box.scope.clone(),
            folder_id: root.id,
            size: 120,
            file_key: "test/key".to_string(),
            created_by: None,
            expires_at: Utc::now(),
            parent_id: None,
            processing_config: Some(processing_config.clone()),
        },
    )
    .await
    .unwrap();

    assert_eq!(task.status, PresignedTaskStatus::Pending);
    assert_eq!(task.name, "test");
    assert_eq!(task.mime, "text/plain");
    assert_eq!(task.document_box, document_box.scope);
    assert_eq!(task.folder_id, root.id);
    assert_eq!(task.size, 120);
    assert_eq!(task.file_key, "test/key");
    assert_eq!(task.created_by, None);
    assert_eq!(task.parent_id, None);
    assert_eq!(
        task.processing_config,
        Some(Json(processing_config.clone()))
    );

    let result = PresignedUploadTask::find(&db, &document_box.scope, task.id)
        .await
        .unwrap();

    assert_eq!(result, Some(task));
}

/// Tests that a presigned upload task can have its status changed
#[tokio::test]
async fn test_presigned_upload_task_set_status() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, root) = make_test_document_box(&db, "test", None).await;

    let mut task = PresignedUploadTask::create(
        &db,
        CreatePresignedUploadTask {
            name: "test".to_string(),
            mime: "text/plain".to_string(),
            document_box: document_box.scope.clone(),
            folder_id: root.id,
            ..Default::default()
        },
    )
    .await
    .unwrap();

    assert_eq!(task.status, PresignedTaskStatus::Pending);

    task.set_status(
        &db,
        PresignedTaskStatus::Failed {
            error: "error".to_string(),
        },
    )
    .await
    .unwrap();

    assert_eq!(
        task.status,
        PresignedTaskStatus::Failed {
            error: "error".to_string()
        }
    );

    let result = PresignedUploadTask::find(&db, &document_box.scope, task.id)
        .await
        .unwrap();

    assert_eq!(result, Some(task.clone()));

    task.set_status(
        &db,
        PresignedTaskStatus::Completed {
            file_id: Uuid::nil(),
        },
    )
    .await
    .unwrap();

    assert_eq!(
        task.status,
        PresignedTaskStatus::Completed {
            file_id: Uuid::nil(),
        },
    );

    let result = PresignedUploadTask::find(&db, &document_box.scope, task.id)
        .await
        .unwrap();

    assert_eq!(result, Some(task));
}

/// Tests that a presigned task can be found
#[tokio::test]
async fn test_presigned_upload_task_find() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, root) = make_test_document_box(&db, "test", None).await;

    let task = PresignedUploadTask::create(
        &db,
        CreatePresignedUploadTask {
            name: "test".to_string(),
            mime: "text/plain".to_string(),
            document_box: document_box.scope.clone(),
            folder_id: root.id,
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let result = PresignedUploadTask::find(&db, &document_box.scope, task.id)
        .await
        .unwrap();

    assert_eq!(result, Some(task));

    let result = PresignedUploadTask::find(&db, &document_box.scope, Uuid::nil())
        .await
        .unwrap();

    assert_eq!(result, None);
}

/// Tests that expired presigned upload tasks can be found
#[tokio::test]
async fn test_presigned_upload_task_find_expired() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, root) = make_test_document_box(&db, "test", None).await;

    let tasks = PresignedUploadTask::find_expired(&db, Utc::now())
        .await
        .unwrap();

    assert!(tasks.is_empty());

    let task = PresignedUploadTask::create(
        &db,
        CreatePresignedUploadTask {
            document_box: document_box.scope.clone(),
            folder_id: root.id,
            file_key: "test/key".to_string(),
            expires_at: Utc::now().checked_sub_days(Days::new(1)).unwrap(),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let tasks = PresignedUploadTask::find_expired(&db, Utc::now())
        .await
        .unwrap();

    assert_eq!(tasks.len(), 1);
    let other_task = tasks
        .iter()
        .find(|item| item.id == task.id)
        .expect("task should exist");
    assert_eq!(other_task, &task);

    let tasks =
        PresignedUploadTask::find_expired(&db, Utc::now().checked_sub_days(Days::new(15)).unwrap())
            .await
            .unwrap();

    assert!(tasks.is_empty());
}

/// Tests that presigned upload tasks can be found using the file key
#[tokio::test]
async fn test_presigned_upload_task_find_by_file_key() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, root) = make_test_document_box(&db, "test", None).await;

    let task = PresignedUploadTask::create(
        &db,
        CreatePresignedUploadTask {
            document_box: document_box.scope.clone(),
            folder_id: root.id,
            file_key: "test/key".to_string(),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let result = PresignedUploadTask::find_by_file_key(&db, "test/key")
        .await
        .unwrap();

    assert_eq!(result, Some(task));

    let result = PresignedUploadTask::find_by_file_key(&db, "test/unknown/key")
        .await
        .unwrap();

    assert_eq!(result, None);
}

/// Tests that a presigned upload tasks can be deleted
#[tokio::test]
async fn test_presigned_upload_task_delete() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, root) = make_test_document_box(&db, "test", None).await;

    let task = PresignedUploadTask::create(
        &db,
        CreatePresignedUploadTask {
            name: "test".to_string(),
            mime: "text/plain".to_string(),
            document_box: document_box.scope.clone(),
            folder_id: root.id,
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let other_task = PresignedUploadTask::create(
        &db,
        CreatePresignedUploadTask {
            name: "test_2".to_string(),
            mime: "text/plain".to_string(),
            document_box: document_box.scope.clone(),
            folder_id: root.id,
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // Should be able to delete the task
    let result = PresignedUploadTask::delete(&db, task.id).await.unwrap();
    assert_eq!(result.rows_affected(), 1);

    // Shouldn't be able to find the task after deletion
    let result = PresignedUploadTask::find(&db, &document_box.scope, task.id)
        .await
        .unwrap();
    assert_eq!(result, None);

    // No results should
    let result = PresignedUploadTask::delete(&db, task.id).await.unwrap();
    assert_eq!(result.rows_affected(), 0);

    // Other task should still exist
    let result = PresignedUploadTask::find(&db, &document_box.scope, other_task.id)
        .await
        .unwrap();
    assert_eq!(result, Some(other_task.clone()));
}
