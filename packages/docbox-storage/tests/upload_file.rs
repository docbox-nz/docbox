use docbox_storage::UploadFileOptions;

use crate::common::minio::{test_minio_container, test_storage_factory};

mod common;

/// Tests uploading a file succeeds
#[tokio::test]
async fn test_upload_file_minio() {
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

    let contents = storage
        .get_file("test.txt")
        .await
        .unwrap()
        .collect_bytes()
        .await
        .unwrap();

    assert_eq!(contents.as_ref(), b"test");
}

/// Tests uploading a file with a duplicate key will override the existing content
#[tokio::test]
async fn test_upload_file_duplicate_key_override_minio() {
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

    let contents = storage
        .get_file("test.txt")
        .await
        .unwrap()
        .collect_bytes()
        .await
        .unwrap();

    assert_eq!(contents.as_ref(), b"test");

    storage
        .upload_file(
            "test.txt",
            "test2".into(),
            UploadFileOptions {
                content_type: "text/plain".to_string(),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    let contents = storage
        .get_file("test.txt")
        .await
        .unwrap()
        .collect_bytes()
        .await
        .unwrap();

    assert_eq!(contents.as_ref(), b"test2");
}
