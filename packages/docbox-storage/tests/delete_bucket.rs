use crate::common::minio::{test_minio_container, test_storage_factory};

mod common;

/// Tests that a bucket can be deleted after being created
#[tokio::test]
async fn test_delete_bucket_minio() {
    let container = test_minio_container().await;
    let storage_factory = test_storage_factory(&container).await;
    let storage = storage_factory.create_test_layer();

    storage.create_bucket().await.unwrap();
    storage.delete_bucket().await.unwrap();

    // Bucket should not exist
    assert!(!storage.bucket_exists().await.unwrap());
}

/// Tests that a bucket can be "deleted" safely twice without throwing
/// an error if it did not exist
#[tokio::test]
async fn test_delete_bucket_minio_safe_double_delete() {
    let container = test_minio_container().await;
    let storage_factory = test_storage_factory(&container).await;
    let storage = storage_factory.create_test_layer();

    storage.create_bucket().await.unwrap();
    storage.delete_bucket().await.unwrap();
    storage.delete_bucket().await.unwrap();

    // Bucket should not exist
    assert!(!storage.bucket_exists().await.unwrap());
}

/// Tests that a bucket can be "deleted" even if it
/// did not exist yet (For graceful deletion if something was partially deleted)
#[tokio::test]
async fn test_delete_bucket_minio_safe_delete_missing() {
    let container = test_minio_container().await;
    let storage_factory = test_storage_factory(&container).await;
    let storage = storage_factory.create_test_layer();

    storage.delete_bucket().await.unwrap();

    // Bucket should not exist
    assert!(!storage.bucket_exists().await.unwrap());
}
