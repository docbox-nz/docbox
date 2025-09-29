use std::{str::FromStr, time::Duration};

use docbox_storage::StorageLayerFactory;
use reqwest::{
    Method,
    header::{HeaderName, HeaderValue},
};
use serde::Serialize;
use tokio::sync::mpsc::UnboundedSender;

use crate::verify::{VerifyOutcome, verify_dummy_tenant};

#[derive(Debug, Clone, Default, Serialize)]
pub struct StorageVerifyOutcome {
    pub create_bucket: VerifyOutcome,
    pub upload_file: VerifyOutcome,
    pub get_file: VerifyOutcome,
    pub delete_file: VerifyOutcome,
    pub create_presigned: VerifyOutcome,
    pub create_presigned_download: VerifyOutcome,
    pub delete_bucket: VerifyOutcome,
    // TODO: add_bucket_notifications, set_bucket_cors_origins
}

/// Verify that a storage backend is working
pub async fn verify_storage(
    storage: &StorageLayerFactory,
    notify: UnboundedSender<StorageVerifyOutcome>,
) -> StorageVerifyOutcome {
    let tenant = verify_dummy_tenant();
    let storage = storage.create_storage_layer(&tenant);

    let mut outcome = StorageVerifyOutcome::default();

    // Create the bucket
    if let Err(error) = storage.create_bucket().await {
        outcome.create_bucket = VerifyOutcome::Failure {
            message: error.to_string(),
        };
        _ = notify.send(outcome.clone());
        return outcome;
    }

    outcome.create_bucket = VerifyOutcome::Success;
    _ = notify.send(outcome.clone());

    let test_key = "test_file_do_not_use";
    let test_content = "test_content".as_bytes();

    // Test uploading a file
    if let Err(error) = storage
        .upload_file(test_key, "text/plain".to_string(), test_content.into())
        .await
    {
        outcome.upload_file = VerifyOutcome::Failure {
            message: error.to_string(),
        };
        _ = notify.send(outcome.clone());
        return outcome;
    }

    outcome.upload_file = VerifyOutcome::Success;
    _ = notify.send(outcome.clone());

    // Test retrieving a file
    let file_stream = match storage.get_file(test_key).await {
        Ok(value) => value,
        Err(error) => {
            outcome.get_file = VerifyOutcome::Failure {
                message: error.to_string(),
            };
            _ = notify.send(outcome.clone());
            return outcome;
        }
    };

    {
        // Read the file contents stream to end
        let output = match file_stream.collect_bytes().await {
            Ok(value) => value,
            Err(error) => {
                outcome.get_file = VerifyOutcome::Failure {
                    message: error.to_string(),
                };
                _ = notify.send(outcome.clone());
                return outcome;
            }
        };

        // Ensure the content matches
        if !output.to_vec().eq(test_content) {
            outcome.get_file = VerifyOutcome::Failure {
                message: "written test file content did not match read-back value".to_string(),
            };
            _ = notify.send(outcome.clone());
            return outcome;
        }
    }

    outcome.get_file = VerifyOutcome::Success;
    _ = notify.send(outcome.clone());

    // Test deleting a file
    if let Err(error) = storage.delete_file(test_key).await {
        outcome.delete_file = VerifyOutcome::Failure {
            message: error.to_string(),
        };
        _ = notify.send(outcome.clone());
        return outcome;
    }

    outcome.delete_file = VerifyOutcome::Success;
    _ = notify.send(outcome.clone());

    // Create a presigned upload
    let (presigned, _presigned_expires_at) = match storage
        .create_presigned(test_key, test_content.len() as i64)
        .await
    {
        Ok(value) => value,
        Err(error) => {
            outcome.create_presigned = VerifyOutcome::Failure {
                message: error.to_string(),
            };
            _ = notify.send(outcome.clone());
            return outcome;
        }
    };

    // Attempt a presigned upload
    {
        let client = reqwest::Client::new();
        let method = match Method::from_str(presigned.method()) {
            Ok(value) => value,
            Err(error) => {
                outcome.create_presigned = VerifyOutcome::Failure {
                    message: error.to_string(),
                };
                _ = notify.send(outcome.clone());
                _ = storage.delete_file(test_key).await;
                return outcome;
            }
        };

        let mut headers = reqwest::header::HeaderMap::new();
        for (name, value) in presigned.headers() {
            let header_name = match HeaderName::from_str(name) {
                Ok(value) => value,
                Err(error) => {
                    outcome.create_presigned = VerifyOutcome::Failure {
                        message: error.to_string(),
                    };
                    _ = notify.send(outcome.clone());
                    _ = storage.delete_file(test_key).await;
                    return outcome;
                }
            };
            let header_value = match HeaderValue::from_str(value) {
                Ok(value) => value,
                Err(error) => {
                    outcome.create_presigned = VerifyOutcome::Failure {
                        message: error.to_string(),
                    };
                    _ = notify.send(outcome.clone());
                    _ = storage.delete_file(test_key).await;
                    return outcome;
                }
            };

            headers.insert(header_name, header_value);
        }

        let response = match client
            .request(method, presigned.uri())
            .headers(headers)
            .body(test_content.to_vec())
            .send()
            .await
        {
            Ok(value) => value,
            Err(error) => {
                outcome.create_presigned = VerifyOutcome::Failure {
                    message: error.to_string(),
                };
                _ = notify.send(outcome.clone());
                _ = storage.delete_file(test_key).await;
                return outcome;
            }
        };

        let _response = match response.error_for_status() {
            Ok(value) => value,
            Err(error) => {
                outcome.create_presigned = VerifyOutcome::Failure {
                    message: error.to_string(),
                };
                _ = notify.send(outcome.clone());
                _ = storage.delete_file(test_key).await;
                return outcome;
            }
        };
    }

    outcome.create_presigned = VerifyOutcome::Success;
    _ = notify.send(outcome.clone());

    // Create a presigned download
    let (presigned, _presigned_expires_at) = match storage
        .create_presigned_download(test_key, Duration::from_secs(120))
        .await
    {
        Ok(value) => value,
        Err(error) => {
            outcome.create_presigned_download = VerifyOutcome::Failure {
                message: error.to_string(),
            };
            _ = notify.send(outcome.clone());
            return outcome;
        }
    };

    // Attempt a presigned download
    {
        let client = reqwest::Client::new();
        let method = match Method::from_str(presigned.method()) {
            Ok(value) => value,
            Err(error) => {
                outcome.create_presigned_download = VerifyOutcome::Failure {
                    message: error.to_string(),
                };
                _ = notify.send(outcome.clone());
                _ = storage.delete_file(test_key).await;
                return outcome;
            }
        };

        let mut headers = reqwest::header::HeaderMap::new();
        for (name, value) in presigned.headers() {
            let header_name = match HeaderName::from_str(name) {
                Ok(value) => value,
                Err(error) => {
                    outcome.create_presigned_download = VerifyOutcome::Failure {
                        message: error.to_string(),
                    };
                    _ = notify.send(outcome.clone());
                    _ = storage.delete_file(test_key).await;
                    return outcome;
                }
            };
            let header_value = match HeaderValue::from_str(value) {
                Ok(value) => value,
                Err(error) => {
                    outcome.create_presigned_download = VerifyOutcome::Failure {
                        message: error.to_string(),
                    };
                    _ = notify.send(outcome.clone());
                    _ = storage.delete_file(test_key).await;
                    return outcome;
                }
            };

            headers.insert(header_name, header_value);
        }

        let response = match client
            .request(method, presigned.uri())
            .headers(headers)
            .send()
            .await
        {
            Ok(value) => value,
            Err(error) => {
                outcome.create_presigned_download = VerifyOutcome::Failure {
                    message: error.to_string(),
                };
                _ = notify.send(outcome.clone());
                _ = storage.delete_file(test_key).await;
                return outcome;
            }
        };

        let response = match response.error_for_status() {
            Ok(value) => value,
            Err(error) => {
                outcome.create_presigned_download = VerifyOutcome::Failure {
                    message: error.to_string(),
                };
                _ = notify.send(outcome.clone());
                _ = storage.delete_file(test_key).await;
                return outcome;
            }
        };

        let response = match response.bytes().await {
            Ok(value) => value,
            Err(error) => {
                outcome.create_presigned_download = VerifyOutcome::Failure {
                    message: error.to_string(),
                };
                _ = notify.send(outcome.clone());
                _ = storage.delete_file(test_key).await;
                return outcome;
            }
        };

        // Ensure the content matches
        if !response.to_vec().eq(test_content) {
            outcome.create_presigned_download = VerifyOutcome::Failure {
                message: "written presigned test file content did not match read-back value"
                    .to_string(),
            };
            _ = notify.send(outcome.clone());
            _ = storage.delete_file(test_key).await;
            return outcome;
        }
    }

    outcome.create_presigned_download = VerifyOutcome::Success;
    _ = notify.send(outcome.clone());
    _ = storage.delete_file(test_key).await;

    // Delete the bucket
    if let Err(error) = storage.delete_bucket().await {
        outcome.delete_bucket = VerifyOutcome::Failure {
            message: error.to_string(),
        };
        _ = notify.send(outcome.clone());
        return outcome;
    }

    outcome.delete_bucket = VerifyOutcome::Success;
    _ = notify.send(outcome.clone());

    outcome
}
