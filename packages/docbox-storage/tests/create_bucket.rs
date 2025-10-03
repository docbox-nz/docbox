use crate::common::minio::test_minio;

mod common;

/// Tests creating a bucket succeeds
#[tokio::test]
async fn test_create_bucket_minio() {
    let (_container, storage) = test_minio().await;
    storage.create_bucket().await.unwrap();
}

/// Tests that creating a duplicate bucket is silently handled
#[tokio::test]
async fn test_create_duplicate_bucket_minio() {
    let (_container, storage) = test_minio().await;
    storage.create_bucket().await.unwrap();
    storage.create_bucket().await.unwrap();
}
