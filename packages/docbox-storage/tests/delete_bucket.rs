use crate::common::minio::test_minio;

mod common;

#[tokio::test]
async fn test_delete_bucket_minio() {
    let (_container, storage) = test_minio().await;
    storage.create_bucket().await.unwrap();
    storage.delete_bucket().await.unwrap();
}
