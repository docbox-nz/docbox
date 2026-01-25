use crate::common::minio::{test_minio_container, test_storage_factory};

mod common;

/// Tests creating a bucket succeeds
#[tokio::test]
async fn test_create_bucket_minio() {
    let container = test_minio_container().await;
    let storage_factory = test_storage_factory(&container).await;
    let storage = storage_factory.create_test_layer();

    storage.create_bucket().await.unwrap();
}

/// Tests that creating a duplicate bucket is silently handled
#[tokio::test]
async fn test_create_duplicate_bucket_minio() {
    let container = test_minio_container().await;
    let storage_factory = test_storage_factory(&container).await;
    let storage = storage_factory.create_test_layer();

    storage.create_bucket().await.unwrap();
    storage.create_bucket().await.unwrap();
}
