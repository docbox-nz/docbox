use crate::common::{
    minio::{test_minio_container, test_storage_factory},
    tenant::test_tenant,
};

mod common;

/// Tests that bucket_exists() reports the correct state for a bucket
#[tokio::test]
async fn test_bucket_exists_minio() {
    let container = test_minio_container().await;
    let storage_factory = test_storage_factory(&container).await;
    let storage = storage_factory.create_storage_layer(&test_tenant());

    storage.create_bucket().await.unwrap();

    let exists = storage.bucket_exists().await.unwrap();
    assert!(exists);

    storage.delete_bucket().await.unwrap();

    let exists = storage.bucket_exists().await.unwrap();
    assert!(!exists);
}
