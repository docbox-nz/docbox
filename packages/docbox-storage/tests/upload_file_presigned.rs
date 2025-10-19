use aws_sdk_s3::presigning::PresignedRequest;
use reqwest::header::{HeaderName, HeaderValue};
use std::str::FromStr;

use crate::common::{
    minio::{test_minio_container, test_storage_factory},
    tenant::test_tenant,
};

mod common;

/// Test helper to perform presigned uploads
async fn presigned_upload(request: PresignedRequest, data: &'static [u8]) {
    let client = reqwest::Client::default();
    client
        .request(request.method().parse().unwrap(), request.uri().to_string())
        .body(data)
        .headers(
            request
                .headers()
                .map(|(key, value)| {
                    (
                        HeaderName::from_str(key).unwrap(),
                        HeaderValue::from_str(value).unwrap(),
                    )
                })
                .collect(),
        )
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
}

/// Tests uploading a file using presigned uploads succeeds
#[tokio::test]
async fn test_upload_file_presigned_minio() {
    let container = test_minio_container().await;
    let storage_factory = test_storage_factory(&container).await;
    let storage = storage_factory.create_storage_layer(&test_tenant());

    let data = b"test";

    storage.create_bucket().await.unwrap();
    let (request, _date) = storage
        .create_presigned("test.txt", data.len() as i64)
        .await
        .unwrap();

    presigned_upload(request, data).await;

    let contents = storage
        .get_file("test.txt")
        .await
        .unwrap()
        .collect_bytes()
        .await
        .unwrap();

    assert_eq!(contents.as_ref(), b"test");
}

/// Tests uploading a file using presigned uploads with a duplicate key will override the existing ocvntent
#[tokio::test]
async fn test_upload_file_duplicate_key_override_minio() {
    let container = test_minio_container().await;
    let storage_factory = test_storage_factory(&container).await;
    let storage = storage_factory.create_storage_layer(&test_tenant());

    // First write
    {
        let data = b"test";

        storage.create_bucket().await.unwrap();
        let (request, _date) = storage
            .create_presigned("test.txt", data.len() as i64)
            .await
            .unwrap();

        presigned_upload(request, data).await;

        let contents = storage
            .get_file("test.txt")
            .await
            .unwrap()
            .collect_bytes()
            .await
            .unwrap();

        assert_eq!(contents.as_ref(), b"test");
    }

    // Second write
    {
        let data2 = b"test2";

        let (request, _date) = storage
            .create_presigned("test.txt", data2.len() as i64)
            .await
            .unwrap();

        presigned_upload(request, data2).await;

        let contents = storage
            .get_file("test.txt")
            .await
            .unwrap()
            .collect_bytes()
            .await
            .unwrap();

        assert_eq!(contents.as_ref(), b"test2");
    }
}
