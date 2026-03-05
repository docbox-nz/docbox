use docbox_storage::UploadFileOptions;

use crate::common::minio::{test_minio_container, test_storage_factory};

mod common;

/// Tests deleting a file succeeds
#[tokio::test]
async fn test_delete_file_minio() {
    let container = test_minio_container().await;
    let storage_factory = test_storage_factory(&container).await;
    let storage = storage_factory.create_test_layer();

    storage.create_bucket().await.unwrap();
    storage
        .upload_file(
            "test.txt",
            "test".into(),
            UploadFileOptions {
                content_type: "text/plain".to_string(),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    storage.delete_file("test.txt").await.unwrap();
}

/// Tests deleting a unknown file succeeds
#[tokio::test]
async fn test_delete_file_unknown_minio() {
    let container = test_minio_container().await;
    let storage_factory = test_storage_factory(&container).await;
    let storage = storage_factory.create_test_layer();

    storage.create_bucket().await.unwrap();

    storage.delete_file("test.txt").await.unwrap();
    storage.delete_file("test.txt").await.unwrap();
}
