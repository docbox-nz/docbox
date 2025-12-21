//! # Convert Lambda
//!
//! Lambda based file conversion server https://github.com/jacobtread/office-convert-lambda backend
//! for performing office file conversion
//!
//! ## Environment Variables
//!
//! * `DOCBOX_CONVERT_LAMBDA_TMP_BUCKET` - S3 bucket to store the temporary input and output files from conversion
//! * `DOCBOX_CONVERT_LAMBDA_FUNCTION_NAME` - The name or ARN of the Lambda function, version, or alias.
//! * `DOCBOX_CONVERT_LAMBDA_QUALIFIER` - Optionally specify a version or alias to invoke a published version of the function.
//! * `DOCBOX_CONVERT_LAMBDA_TENANT_ID` - Optional identifier of the tenant in a multi-tenant Lambda function.
//! * `DOCBOX_CONVERT_LAMBDA_RETRY_ATTEMPTS` - Maximum number of times to retry on unexpected failures
//! * `DOCBOX_CONVERT_LAMBDA_RETRY_WAIT` - Delay to wait between each retry attempt

use std::{str::FromStr, time::Duration};

use crate::office::libreoffice::is_known_libreoffice_pdf_convertable;

use super::{ConvertToPdf, PdfConvertError};
use aws_config::SdkConfig;
use bytes::Bytes;
use docbox_database::sqlx::types::Uuid;
use docbox_storage::{StorageLayerError, StorageLayerFactory, TenantStorageLayer};
use office_convert_lambda_client::{ConvertError, OfficeConvertLambda, OfficeConvertLambdaOptions};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfficeConvertLambdaConfig {
    /// The name or ARN of the Lambda function, version, or alias.
    pub function_name: String,
    /// Specify a version or alias to invoke a published version of the function.
    pub qualifier: Option<String>,
    /// The identifier of the tenant in a multi-tenant Lambda function.
    pub tenant_id: Option<String>,
    /// Number of retry attempts to perform
    pub retry_attempts: usize,
    /// Time to wait between retry attempts
    pub retry_wait: Duration,
    /// Temporary bucket to use for lambda input and output files
    pub tmp_bucket: String,
}

#[derive(Debug, Error)]
pub enum OfficeConvertLambdaConfigError {
    #[error("missing DOCBOX_CONVERT_LAMBDA_TMP_BUCKET environment variable")]
    MissingTempBucket,
    #[error("missing DOCBOX_CONVERT_LAMBDA_FUNCTION_NAME environment variable")]
    MissingFunctionName,
    #[error("DOCBOX_CONVERT_LAMBDA_RETRY_ATTEMPTS must be a number")]
    InvalidRetryAttempts(<usize as FromStr>::Err),
    #[error("DOCBOX_CONVERT_LAMBDA_RETRY_WAIT must be a number in seconds: {0}")]
    InvalidRetryWait(<u64 as FromStr>::Err),
}

impl OfficeConvertLambdaConfig {
    pub fn from_env() -> Result<OfficeConvertLambdaConfig, OfficeConvertLambdaConfigError> {
        let tmp_bucket = std::env::var("DOCBOX_CONVERT_LAMBDA_TMP_BUCKET")
            .map_err(|_| OfficeConvertLambdaConfigError::MissingTempBucket)?;

        let function_name = std::env::var("DOCBOX_CONVERT_LAMBDA_FUNCTION_NAME")
            .map_err(|_| OfficeConvertLambdaConfigError::MissingFunctionName)?;

        let qualifier = std::env::var("DOCBOX_CONVERT_LAMBDA_QUALIFIER").ok();
        let tenant_id = std::env::var("DOCBOX_CONVERT_LAMBDA_TENANT_ID").ok();

        let retry_attempts = match std::env::var("DOCBOX_CONVERT_LAMBDA_RETRY_ATTEMPTS") {
            Ok(retry_attempts) => retry_attempts
                .parse::<usize>()
                .map_err(OfficeConvertLambdaConfigError::InvalidRetryAttempts)?,
            Err(_) => 3,
        };

        let retry_wait = match std::env::var("DOCBOX_CONVERT_LAMBDA_RETRY_WAIT") {
            Ok(retry_wait) => retry_wait
                .parse::<u64>()
                .map_err(OfficeConvertLambdaConfigError::InvalidRetryWait)
                .map(Duration::from_secs)?,
            Err(_) => Duration::from_secs(1),
        };

        Ok(OfficeConvertLambdaConfig {
            tmp_bucket,
            function_name,
            qualifier,
            tenant_id,
            retry_attempts,
            retry_wait,
        })
    }
}

/// Variant of [ConvertToPdf] that uses LibreOffice through a
/// office-converter server for the conversion
#[derive(Clone)]
pub struct OfficeConverterLambda {
    client: OfficeConvertLambda,
    storage: TenantStorageLayer,
}

#[derive(Debug, Error)]
pub enum OfficeConvertLambdaError {
    /// Error on the storage layer
    #[error(transparent)]
    Storage(#[from] StorageLayerError),

    /// Error when converting
    #[error(transparent)]
    Convert(#[from] Box<ConvertError>),
}

impl OfficeConverterLambda {
    pub fn new(client: OfficeConvertLambda, storage: TenantStorageLayer) -> Self {
        Self { client, storage }
    }

    pub fn from_config(
        aws_config: &SdkConfig,
        storage: &StorageLayerFactory,
        config: OfficeConvertLambdaConfig,
    ) -> Result<Self, OfficeConvertLambdaError> {
        let client = aws_sdk_lambda::Client::new(aws_config);
        let storage = storage.create_storage_layer_bucket(config.tmp_bucket);

        Ok(Self {
            client: OfficeConvertLambda::new(
                client,
                OfficeConvertLambdaOptions {
                    function_name: config.function_name,
                    qualifier: config.qualifier,
                    tenant_id: config.tenant_id,
                    retry_attempts: config.retry_attempts,
                    retry_wait: config.retry_wait,
                },
            ),
            storage,
        })
    }
}

impl ConvertToPdf for OfficeConverterLambda {
    async fn convert_to_pdf(&self, file_bytes: Bytes) -> Result<Bytes, PdfConvertError> {
        let bucket_name = self.storage.bucket_name();
        let input_key = Uuid::new_v4().simple().to_string();
        let output_key = Uuid::new_v4().simple().to_string();

        tracing::debug!("uploading file for conversion");

        // Upload the file to S3
        self.storage
            .upload_file(
                &input_key,
                "application/octet-stream".to_string(),
                file_bytes,
            )
            .await
            .map_err(OfficeConvertLambdaError::Storage)?;

        tracing::debug!("calling conversion lambda");

        let result = self
            .client
            .convert(office_convert_lambda_client::ConvertRequest {
                source_bucket: bucket_name.clone(),
                source_key: input_key.clone(),
                dest_bucket: bucket_name.clone(),
                dest_key: output_key.clone(),
            })
            .await;

        tracing::debug!("conversion complete");

        // Delete the input file after completion
        self.storage
            .delete_file(&input_key)
            .await
            .map_err(OfficeConvertLambdaError::Storage)?;

        match result {
            Ok(_) => {
                tracing::debug!("reading converted file");

                // Read the output file back
                let output_bytes = self
                    .storage
                    .get_file(&output_key)
                    .await
                    .map_err(OfficeConvertLambdaError::Storage)?
                    .collect_bytes()
                    .await
                    .map_err(OfficeConvertLambdaError::Storage)?;

                Ok(output_bytes)
            }
            Err(error) => {
                tracing::error!(?error, "failed to convert file");
                Err(match error {
                    ConvertError::Lambda(err) if err.reason.as_str() == "FILE_LIKELY_ENCRYPTED" => {
                        PdfConvertError::EncryptedDocument
                    }
                    ConvertError::Lambda(err) if err.reason.as_str() == "FILE_LIKELY_CORRUPTED" => {
                        PdfConvertError::MalformedDocument
                    }
                    err => PdfConvertError::ConversionFailedLambda(
                        OfficeConvertLambdaError::Convert(Box::new(err)),
                    ),
                })
            }
        }
    }

    fn is_convertable(&self, mime: &mime::Mime) -> bool {
        is_known_libreoffice_pdf_convertable(mime)
    }
}
