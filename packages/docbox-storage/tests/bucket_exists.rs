use crate::common::minio::test_minio;

mod common;

/// Tests that bucket_exists() reports the correct state for a bucket
#[tokio::test]
async fn test_bucket_exists_minio() {
    let (_container, storage) = test_minio().await;

    storage.create_bucket().await.unwrap();

    let exists = storage.bucket_exists().await.unwrap();
    assert!(exists);

    storage.delete_bucket().await.unwrap();

    let exists = storage.bucket_exists().await.unwrap();
    assert!(!exists);
}
