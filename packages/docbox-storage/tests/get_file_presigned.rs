use crate::common::minio::{test_minio_container, test_storage_factory};
use aws_sdk_s3::presigning::PresignedRequest;
use reqwest::{
    Response,
    header::{HeaderName, HeaderValue},
};
use std::{str::FromStr, time::Duration};

mod common;

/// Test helper to perform presigned requests
async fn presigned_request(request: PresignedRequest) -> Response {
    let client = reqwest::Client::default();
    client
        .request(request.method().parse().unwrap(), request.uri().to_string())
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
}

/// Tests getting a file's content succeeds and matches the uploaded content
#[tokio::test]
async fn test_get_file_presigned_minio() {
    let container = test_minio_container().await;
    let storage_factory = test_storage_factory(&container).await;
    let storage = storage_factory.create_test_layer();

    storage.create_bucket().await.unwrap();
    storage
        .upload_file("test.txt", "text/plain".to_string(), "test".into())
        .await
        .unwrap();

    let (request, _date) = storage
        .create_presigned_download("test.txt", Duration::from_secs(60))
        .await
        .unwrap();

    let contents = presigned_request(request)
        .await
        .error_for_status()
        .unwrap()
        .bytes()
        .await
        .unwrap();
    assert_eq!(contents.as_ref(), b"test");
}

/// Tests getting a presigned download of an unknown file fails
///
/// Creating the presigned upload will not fail with minio but retrieving the
/// resource will fail with a 404
#[tokio::test]
async fn test_get_unknown_file_presigned_minio() {
    let container = test_minio_container().await;
    let storage_factory = test_storage_factory(&container).await;
    let storage = storage_factory.create_test_layer();

    storage.create_bucket().await.unwrap();
    let (request, _date) = storage
        .create_presigned_download("test.txt", Duration::from_secs(60))
        .await
        .unwrap();

    presigned_request(request)
        .await
        .error_for_status()
        .unwrap_err();
}
