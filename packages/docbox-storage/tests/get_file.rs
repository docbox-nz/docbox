use crate::common::{
    minio::{test_minio_container, test_storage_factory},
    tenant::test_tenant,
};

mod common;

/// Tests getting a file's content succeeds and matches the uploaded content
#[tokio::test]
async fn test_get_file_minio() {
    let container = test_minio_container().await;
    let storage_factory = test_storage_factory(&container).await;
    let storage = storage_factory.create_storage_layer(&test_tenant());

    storage.create_bucket().await.unwrap();
    storage
        .upload_file("test.txt", "text/plain".to_string(), "test".into())
        .await
        .unwrap();

    let contents = storage
        .get_file("test.txt")
        .await
        .unwrap()
        .collect_bytes()
        .await
        .unwrap();

    assert_eq!(contents.as_ref(), b"test");
}

/// Tests getting the contents of an unknown file fails
#[tokio::test]
async fn test_get_unknown_file_minio() {
    let container = test_minio_container().await;
    let storage_factory = test_storage_factory(&container).await;
    let storage = storage_factory.create_storage_layer(&test_tenant());

    storage.create_bucket().await.unwrap();
    storage.get_file("test.txt").await.unwrap_err();
}
