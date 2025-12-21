//! # S3 Storage Backend
//!
//! Storage backend backed by a [S3](https://docs.aws.amazon.com/s3/) compatible storage solution (AWS S3, MinIO, ...etc)
//!
//! # Environment Variables
//!
//! * `DOCBOX_S3_ENDPOINT` - URL to use when using a custom S3 endpoint
//! * `DOCBOX_S3_EXTERNAL_ENDPOINT` - Alternative "external" user facing endpoint, useful when running the server in docker with a different endpoint
//! * `DOCBOX_S3_ACCESS_KEY_ID` - Access key ID when using a custom S3 endpoint
//! * `DOCBOX_S3_ACCESS_KEY_SECRET` - Access key secret when using a custom S3 endpoint

use crate::{CreateBucketOutcome, FileStream, StorageLayerError, StorageLayerImpl};
use aws_config::SdkConfig;
use aws_sdk_s3::{
    config::Credentials,
    error::SdkError,
    operation::{
        create_bucket::CreateBucketError, delete_bucket::DeleteBucketError,
        delete_object::DeleteObjectError, get_object::GetObjectError, head_bucket::HeadBucketError,
        put_bucket_cors::PutBucketCorsError,
        put_bucket_notification_configuration::PutBucketNotificationConfigurationError,
        put_object::PutObjectError,
    },
    presigning::{PresignedRequest, PresigningConfig},
    primitives::ByteStream,
    types::{
        BucketLocationConstraint, CorsConfiguration, CorsRule, CreateBucketConfiguration,
        NotificationConfiguration, QueueConfiguration,
    },
};
use bytes::Bytes;
use chrono::{DateTime, TimeDelta, Utc};
use futures::Stream;
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt::Debug, time::Duration};
use thiserror::Error;

type S3Client = aws_sdk_s3::Client;

/// Configuration for the S3 storage layer
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct S3StorageLayerFactoryConfig {
    /// Endpoint to use for requests
    pub endpoint: S3Endpoint,
}

/// Errors that could occur when loading the S3 storage layer configuration
#[derive(Debug, Error)]
pub enum S3StorageLayerFactoryConfigError {
    /// Using a custom endpoint but didn't specify the access key ID
    #[error("cannot use DOCBOX_S3_ENDPOINT without specifying DOCBOX_S3_ACCESS_KEY_ID")]
    MissingAccessKeyId,

    /// Using a custom endpoint but didn't specify the access key secret
    #[error("cannot use DOCBOX_S3_ENDPOINT without specifying DOCBOX_S3_ACCESS_KEY_SECRET")]
    MissingAccessKeySecret,
}

impl S3StorageLayerFactoryConfig {
    /// Load a [S3StorageLayerFactoryConfig] from the current environment
    pub fn from_env() -> Result<Self, S3StorageLayerFactoryConfigError> {
        let endpoint = S3Endpoint::from_env()?;

        Ok(Self { endpoint })
    }
}

/// Endpoint to use for S3 operations
#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum S3Endpoint {
    /// AWS default endpoint
    Aws,
    /// Custom endpoint (Minio or other compatible)
    Custom {
        /// Endpoint URL
        endpoint: String,
        /// Endpoint to use for external requests (Presigned requests)
        external_endpoint: Option<String>,
        /// Access key ID to use
        access_key_id: String,
        /// Access key secret to use
        access_key_secret: String,
    },
}

impl Debug for S3Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Aws => write!(f, "Aws"),
            Self::Custom { endpoint, .. } => f
                .debug_struct("Custom")
                .field("endpoint", endpoint)
                .finish(),
        }
    }
}

impl S3Endpoint {
    /// Load a [S3Endpoint] from the current environment
    pub fn from_env() -> Result<Self, S3StorageLayerFactoryConfigError> {
        match std::env::var("DOCBOX_S3_ENDPOINT") {
            // Using a custom S3 endpoint
            Ok(endpoint_url) => {
                let access_key_id = std::env::var("DOCBOX_S3_ACCESS_KEY_ID")
                    .map_err(|_| S3StorageLayerFactoryConfigError::MissingAccessKeyId)?;
                let access_key_secret = std::env::var("DOCBOX_S3_ACCESS_KEY_SECRET")
                    .map_err(|_| S3StorageLayerFactoryConfigError::MissingAccessKeySecret)?;

                let external_endpoint = std::env::var("DOCBOX_S3_EXTERNAL_ENDPOINT").ok();

                Ok(S3Endpoint::Custom {
                    endpoint: endpoint_url,
                    external_endpoint,
                    access_key_id,
                    access_key_secret,
                })
            }
            Err(_) => Ok(S3Endpoint::Aws),
        }
    }
}

/// Storage layer factory backend by a S3 compatible service
#[derive(Clone)]
pub struct S3StorageLayerFactory {
    /// Client to access S3
    client: S3Client,
    /// Optional different client for creating presigned external requests
    external_client: Option<S3Client>,
}

impl S3StorageLayerFactory {
    /// Create a [S3StorageLayerFactory] from a config
    pub fn from_config(aws_config: &SdkConfig, config: S3StorageLayerFactoryConfig) -> Self {
        let (client, external_client) = match config.endpoint {
            S3Endpoint::Aws => {
                tracing::debug!("using aws s3 storage layer");
                (S3Client::new(aws_config), None)
            }
            S3Endpoint::Custom {
                endpoint,
                external_endpoint,
                access_key_id,
                access_key_secret,
            } => {
                tracing::debug!("using custom s3 storage layer");
                let credentials = Credentials::new(
                    access_key_id,
                    access_key_secret,
                    None,
                    None,
                    "docbox_key_provider",
                );

                // Enforces the "path" style for S3 bucket access
                let config_builder = aws_sdk_s3::config::Builder::from(aws_config)
                    .force_path_style(true)
                    .endpoint_url(endpoint)
                    .credentials_provider(credentials);

                // Create an external client for external s3 requests if needed
                let external_client = match external_endpoint {
                    Some(external_endpoint) => {
                        let config = config_builder
                            .clone()
                            .endpoint_url(external_endpoint)
                            .build();
                        let client = S3Client::from_conf(config);
                        Some(client)
                    }
                    None => None,
                };

                let config = config_builder.build();
                let client = S3Client::from_conf(config);

                (client, external_client)
            }
        };

        Self {
            client,
            external_client,
        }
    }

    /// Create a [S3StorageLayer] for the provided `bucket_name`
    pub fn create_storage_layer(&self, bucket_name: String) -> S3StorageLayer {
        S3StorageLayer::new(
            self.client.clone(),
            self.external_client.clone(),
            bucket_name,
        )
    }
}

/// Storage layer backend by a S3 compatible service
#[derive(Clone)]
pub struct S3StorageLayer {
    /// Name of the bucket to use
    bucket_name: String,

    /// Client to access S3
    client: S3Client,

    /// Optional different client for creating presigned external requests
    external_client: Option<S3Client>,
}

impl S3StorageLayer {
    /// Create a new S3 storage layer from the client and bucket name
    fn new(client: S3Client, external_client: Option<S3Client>, bucket_name: String) -> Self {
        Self {
            bucket_name,
            client,
            external_client,
        }
    }
}

/// User facing storage errors
///
/// Should not contain the actual error types, these will be logged
/// early, only includes the actual error message
#[derive(Debug, Error)]
pub enum S3StorageError {
    /// AWS region missing
    #[error("invalid server configuration (region)")]
    MissingRegion,

    /// Failed to create a bucket
    #[error("failed to create storage bucket")]
    CreateBucket(SdkError<CreateBucketError>),

    /// Failed to delete a bucket
    #[error("failed to delete storage bucket")]
    DeleteBucket(SdkError<DeleteBucketError>),

    /// Failed to head a bucket
    #[error("failed to get storage bucket")]
    HeadBucket(SdkError<HeadBucketError>),

    /// Failed to store a file in a bucket
    #[error("failed to store file object")]
    PutObject(SdkError<PutObjectError>),

    /// Failed to calculate future unix timestamps
    #[error("failed to calculate expiry timestamp")]
    UnixTimeCalculation,

    /// Failed to create presigned upload
    #[error("failed to create presigned store file object")]
    PutObjectPresigned(SdkError<PutObjectError>),

    /// Failed to create presigned config
    #[error("failed to create presigned config")]
    PresignedConfig,

    /// Failed to create presigned download
    #[error("failed to get presigned store file object")]
    GetObjectPresigned(SdkError<GetObjectError>),

    /// Failed to create the config for the notification queue
    #[error("failed to create bucket notification queue config")]
    QueueConfig,

    /// Failed to setup a notification queue on the bucket
    ///
    /// This error is allowed to expose the inner error details as
    /// it is only used by the management layer and these errors are
    /// helpful for management
    #[error("failed to add bucket notification queue: {0}")]
    PutBucketNotification(SdkError<PutBucketNotificationConfigurationError>),

    /// Failed to make the cors config or rules
    #[error("failed to create bucket cors config")]
    CreateCorsConfig,

    /// Failed to put the bucket cors config
    ///
    /// This error is allowed to expose the inner error details as
    /// it is only used by the management layer and these errors are
    /// helpful for management
    #[error("failed to set bucket cors rules: {0}")]
    PutBucketCors(SdkError<PutBucketCorsError>),

    /// Failed to delete a file object
    #[error("failed to delete file object")]
    DeleteObject(SdkError<DeleteObjectError>),

    /// Failed to get the file storage object
    #[error("failed to get file storage object")]
    GetObject(SdkError<GetObjectError>),
}

impl StorageLayerImpl for S3StorageLayer {
    fn bucket_name(&self) -> String {
        self.bucket_name.clone()
    }

    async fn create_bucket(&self) -> Result<CreateBucketOutcome, StorageLayerError> {
        let bucket_region = self
            .client
            .config()
            .region()
            .ok_or(S3StorageError::MissingRegion)?
            .to_string();

        let constraint = BucketLocationConstraint::from(bucket_region.as_str());

        let cfg = CreateBucketConfiguration::builder()
            .location_constraint(constraint)
            .build();

        if let Err(error) = self
            .client
            .create_bucket()
            .create_bucket_configuration(cfg)
            .bucket(&self.bucket_name)
            .send()
            .await
        {
            let already_exists = error
                .as_service_error()
                .is_some_and(|value| value.is_bucket_already_owned_by_you());

            // Bucket has already been created
            if already_exists {
                tracing::debug!("bucket already exists");
                return Ok(CreateBucketOutcome::Existing);
            }

            tracing::error!(?error, "failed to create bucket");
            return Err(S3StorageError::CreateBucket(error).into());
        }

        Ok(CreateBucketOutcome::New)
    }

    async fn bucket_exists(&self) -> Result<bool, StorageLayerError> {
        if let Err(error) = self
            .client
            .head_bucket()
            .bucket(&self.bucket_name)
            .send()
            .await
        {
            // Handle not found error (In this case its an indicator and not an error)
            if error
                .as_service_error()
                .is_some_and(|error| error.is_not_found())
            {
                return Ok(false);
            }

            return Err(S3StorageError::HeadBucket(error).into());
        }

        Ok(true)
    }

    async fn delete_bucket(&self) -> Result<(), StorageLayerError> {
        if let Err(error) = self
            .client
            .delete_bucket()
            .bucket(&self.bucket_name)
            .send()
            .await
        {
            // Handle the bucket not existing
            // (This is not a failure and indicates the bucket is already deleted)
            if error
                .as_service_error()
                .and_then(|err| err.meta().code())
                .is_some_and(|code| code == "NoSuchBucket")
            {
                tracing::debug!("bucket did not exist");
                return Ok(());
            }

            tracing::error!(?error, "failed to delete bucket");

            return Err(S3StorageError::DeleteBucket(error).into());
        }

        Ok(())
    }

    async fn upload_file(
        &self,
        key: &str,
        content_type: String,
        body: Bytes,
    ) -> Result<(), StorageLayerError> {
        self.client
            .put_object()
            .bucket(&self.bucket_name)
            .content_type(content_type)
            .key(key)
            .body(body.into())
            .send()
            .await
            .map_err(|error| {
                tracing::error!(?error, "failed to store file object");
                S3StorageError::PutObject(error)
            })?;

        Ok(())
    }

    async fn create_presigned(
        &self,
        key: &str,
        size: i64,
    ) -> Result<(PresignedRequest, DateTime<Utc>), StorageLayerError> {
        let expiry_time_minutes = 30;
        let expires_at = Utc::now()
            .checked_add_signed(TimeDelta::minutes(expiry_time_minutes))
            .ok_or(S3StorageError::UnixTimeCalculation)?;

        let client = match self.external_client.as_ref() {
            Some(external_client) => external_client,
            None => &self.client,
        };

        let result = client
            .put_object()
            .bucket(&self.bucket_name)
            .key(key)
            .content_length(size)
            .presigned(
                PresigningConfig::builder()
                    .expires_in(Duration::from_secs(60 * expiry_time_minutes as u64))
                    .build()
                    .map_err(|error| {
                        tracing::error!(?error, "Failed to create presigned store config");
                        S3StorageError::PresignedConfig
                    })?,
            )
            .await
            .map_err(|error| {
                tracing::error!(?error, "failed to create presigned store file object");
                S3StorageError::PutObjectPresigned(error)
            })?;

        Ok((result, expires_at))
    }

    async fn create_presigned_download(
        &self,
        key: &str,
        expires_in: Duration,
    ) -> Result<(PresignedRequest, DateTime<Utc>), StorageLayerError> {
        let expires_at = Utc::now()
            .checked_add_signed(TimeDelta::seconds(expires_in.as_secs() as i64))
            .ok_or(S3StorageError::UnixTimeCalculation)?;

        let client = match self.external_client.as_ref() {
            Some(external_client) => external_client,
            None => &self.client,
        };

        let result = client
            .get_object()
            .bucket(&self.bucket_name)
            .key(key)
            .presigned(PresigningConfig::expires_in(expires_in).map_err(|error| {
                tracing::error!(?error, "failed to create presigned download config");
                S3StorageError::PresignedConfig
            })?)
            .await
            .map_err(|error| {
                tracing::error!(?error, "failed to create presigned download");
                S3StorageError::GetObjectPresigned(error)
            })?;

        Ok((result, expires_at))
    }

    async fn add_bucket_notifications(&self, sqs_arn: &str) -> Result<(), StorageLayerError> {
        // Connect the S3 bucket for file upload notifications
        self.client
            .put_bucket_notification_configuration()
            .bucket(&self.bucket_name)
            .notification_configuration(
                NotificationConfiguration::builder()
                    .set_queue_configurations(Some(vec![
                        QueueConfiguration::builder()
                            .queue_arn(sqs_arn)
                            .events(aws_sdk_s3::types::Event::S3ObjectCreated)
                            .build()
                            .map_err(|error| {
                                tracing::error!(
                                    ?error,
                                    "failed to create bucket notification queue config"
                                );
                                S3StorageError::QueueConfig
                            })?,
                    ]))
                    .build(),
            )
            .send()
            .await
            .map_err(|error| {
                tracing::error!(?error, "failed to add bucket notification queue");
                S3StorageError::PutBucketNotification(error)
            })?;

        Ok(())
    }

    async fn set_bucket_cors_origins(&self, origins: Vec<String>) -> Result<(), StorageLayerError> {
        if let Err(error) = self
            .client
            .put_bucket_cors()
            .bucket(&self.bucket_name)
            .cors_configuration(
                CorsConfiguration::builder()
                    .cors_rules(
                        CorsRule::builder()
                            .allowed_headers("*")
                            .allowed_methods("PUT")
                            .set_allowed_origins(Some(origins))
                            .set_expose_headers(Some(Vec::new()))
                            .build()
                            .map_err(|error| {
                                tracing::error!(?error, "failed to create cors rule");
                                S3StorageError::CreateCorsConfig
                            })?,
                    )
                    .build()
                    .map_err(|error| {
                        tracing::error!(?error, "failed to create cors config");
                        S3StorageError::CreateCorsConfig
                    })?,
            )
            .send()
            .await
        {
            // Handle "NotImplemented" errors (minio does not have CORS support)
            if error
                .raw_response()
                // (501 Not Implemented)
                .is_some_and(|response| response.status().as_u16() == 501)
            {
                tracing::warn!("storage s3 backend does not support PutBucketCors.. skipping..");
                return Ok(());
            }

            tracing::error!(?error, "failed to add bucket cors");
            return Err(S3StorageError::PutBucketCors(error).into());
        };

        Ok(())
    }

    async fn delete_file(&self, key: &str) -> Result<(), StorageLayerError> {
        if let Err(error) = self
            .client
            .delete_object()
            .bucket(&self.bucket_name)
            .key(key)
            .send()
            .await
        {
            // Handle keys that don't exist in the bucket
            // (This is not a failure and indicates the file is already deleted)
            if error
                .as_service_error()
                .and_then(|err| err.source())
                .and_then(|source| source.downcast_ref::<aws_sdk_s3::Error>())
                .is_some_and(|err| matches!(err, aws_sdk_s3::Error::NoSuchKey(_)))
            {
                return Ok(());
            }

            tracing::error!(?error, "failed to delete file object");
            return Err(S3StorageError::DeleteObject(error).into());
        }

        Ok(())
    }

    async fn get_file(&self, key: &str) -> Result<FileStream, StorageLayerError> {
        let object = self
            .client
            .get_object()
            .bucket(&self.bucket_name)
            .key(key)
            .send()
            .await
            .map_err(|error| {
                tracing::error!(?error, "failed to get file storage object");
                S3StorageError::GetObject(error)
            })?;

        let stream = FileStream {
            stream: Box::pin(AwsFileStream { inner: object.body }),
        };

        Ok(stream)
    }
}

/// File stream based on the AWS [ByteStream] type
pub struct AwsFileStream {
    inner: ByteStream,
}

impl AwsFileStream {
    /// Get the underlying stream
    pub fn into_inner(self) -> ByteStream {
        self.inner
    }
}

impl Stream for AwsFileStream {
    type Item = std::io::Result<Bytes>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.get_mut();
        let inner = std::pin::Pin::new(&mut this.inner);
        inner.poll_next(cx).map_err(std::io::Error::other)
    }
}
