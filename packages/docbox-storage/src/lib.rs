#![forbid(unsafe_code)]

use aws_config::SdkConfig;
use aws_sdk_s3::presigning::PresignedRequest;
use bytes::{Buf, Bytes};
use bytes_utils::SegmentedBuf;
use chrono::{DateTime, Utc};
use docbox_database::models::tenant::Tenant;
use futures::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::{pin::Pin, time::Duration};
use thiserror::Error;

pub mod s3;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum StorageLayerFactoryConfig {
    S3(s3::S3StorageLayerFactoryConfig),
}

#[derive(Debug, Error)]
pub enum StorageLayerFactoryConfigError {
    /// Error from the S3 layer config
    #[error(transparent)]
    S3(#[from] s3::S3StorageLayerFactoryConfigError),
}

impl StorageLayerFactoryConfig {
    pub fn from_env() -> Result<Self, StorageLayerFactoryConfigError> {
        s3::S3StorageLayerFactoryConfig::from_env()
            .map(Self::S3)
            .map_err(StorageLayerFactoryConfigError::S3)
    }
}

#[derive(Clone)]
pub enum StorageLayerFactory {
    S3(s3::S3StorageLayerFactory),
}

#[derive(Debug, Error)]
pub enum StorageLayerError {
    /// Error from the S3 layer
    #[error(transparent)]
    S3(Box<s3::S3StorageError>),

    /// Error collecting streamed response bytes
    #[error("failed to collect file contents")]
    CollectBytes,
}

impl From<s3::S3StorageError> for StorageLayerError {
    fn from(value: s3::S3StorageError) -> Self {
        Self::S3(Box::new(value))
    }
}

impl StorageLayerFactory {
    pub fn from_config(aws_config: &SdkConfig, config: StorageLayerFactoryConfig) -> Self {
        match config {
            StorageLayerFactoryConfig::S3(config) => {
                Self::S3(s3::S3StorageLayerFactory::from_config(aws_config, config))
            }
        }
    }

    pub fn create_storage_layer(&self, tenant: &Tenant) -> TenantStorageLayer {
        match self {
            StorageLayerFactory::S3(s3) => {
                let bucket_name = tenant.s3_name.clone();
                let layer = s3.create_storage_layer(bucket_name);
                TenantStorageLayer::S3(layer)
            }
        }
    }
}

#[derive(Clone)]
pub enum TenantStorageLayer {
    /// Storage layer backed by S3
    S3(s3::S3StorageLayer),
}

impl TenantStorageLayer {
    /// Creates the tenant storage bucket
    #[tracing::instrument(skip(self))]
    pub async fn create_bucket(&self) -> Result<(), StorageLayerError> {
        match self {
            TenantStorageLayer::S3(layer) => layer.create_bucket().await,
        }
    }

    /// Checks if the bucket exists
    #[tracing::instrument(skip(self))]
    pub async fn bucket_exists(&self) -> Result<bool, StorageLayerError> {
        match self {
            TenantStorageLayer::S3(layer) => layer.bucket_exists().await,
        }
    }

    /// Deletes the tenant storage bucket
    ///
    /// In the event that the bucket did not exist before calling this
    /// function this is treated as an [`Ok`] result
    #[tracing::instrument(skip(self))]
    pub async fn delete_bucket(&self) -> Result<(), StorageLayerError> {
        match self {
            TenantStorageLayer::S3(layer) => layer.delete_bucket().await,
        }
    }

    /// Create a presigned file upload URL
    #[tracing::instrument(skip(self))]
    pub async fn create_presigned(
        &self,
        key: &str,
        size: i64,
    ) -> Result<(PresignedRequest, DateTime<Utc>), StorageLayerError> {
        match self {
            TenantStorageLayer::S3(layer) => layer.create_presigned(key, size).await,
        }
    }

    /// Create a presigned file download URL
    #[tracing::instrument(skip(self))]
    pub async fn create_presigned_download(
        &self,
        key: &str,
        expires_in: Duration,
    ) -> Result<(PresignedRequest, DateTime<Utc>), StorageLayerError> {
        match self {
            TenantStorageLayer::S3(layer) => layer.create_presigned_download(key, expires_in).await,
        }
    }

    /// Uploads a file to the S3 bucket for the tenant
    #[tracing::instrument(skip(self, body), fields(body_length = body.len()))]
    pub async fn upload_file(
        &self,
        key: &str,
        content_type: String,
        body: Bytes,
    ) -> Result<(), StorageLayerError> {
        match self {
            TenantStorageLayer::S3(layer) => layer.upload_file(key, content_type, body).await,
        }
    }

    /// Add the SNS notification to a bucket
    #[tracing::instrument(skip(self))]
    pub async fn add_bucket_notifications(&self, sns_arn: &str) -> Result<(), StorageLayerError> {
        match self {
            TenantStorageLayer::S3(layer) => layer.add_bucket_notifications(sns_arn).await,
        }
    }

    /// Sets the allowed CORS origins for accessing the storage from the frontend
    #[tracing::instrument(skip(self))]
    pub async fn set_bucket_cors_origins(
        &self,
        origins: Vec<String>,
    ) -> Result<(), StorageLayerError> {
        match self {
            TenantStorageLayer::S3(layer) => layer.set_bucket_cors_origins(origins).await,
        }
    }

    /// Deletes the S3 file
    #[tracing::instrument(skip(self))]
    pub async fn delete_file(&self, key: &str) -> Result<(), StorageLayerError> {
        match self {
            TenantStorageLayer::S3(layer) => layer.delete_file(key).await,
        }
    }

    /// Gets a byte stream for a file from S3
    #[tracing::instrument(skip(self))]
    pub async fn get_file(&self, key: &str) -> Result<FileStream, StorageLayerError> {
        match self {
            TenantStorageLayer::S3(layer) => layer.get_file(key).await,
        }
    }
}

/// Internal trait defining required async implementations for a storage backend
pub(crate) trait StorageLayerImpl {
    async fn create_bucket(&self) -> Result<(), StorageLayerError>;

    async fn bucket_exists(&self) -> Result<bool, StorageLayerError>;

    async fn delete_bucket(&self) -> Result<(), StorageLayerError>;

    async fn create_presigned(
        &self,
        key: &str,
        size: i64,
    ) -> Result<(PresignedRequest, DateTime<Utc>), StorageLayerError>;

    async fn create_presigned_download(
        &self,
        key: &str,
        expires_in: Duration,
    ) -> Result<(PresignedRequest, DateTime<Utc>), StorageLayerError>;

    async fn upload_file(
        &self,
        key: &str,
        content_type: String,
        body: Bytes,
    ) -> Result<(), StorageLayerError>;

    async fn add_bucket_notifications(&self, sns_arn: &str) -> Result<(), StorageLayerError>;

    async fn set_bucket_cors_origins(&self, origins: Vec<String>) -> Result<(), StorageLayerError>;

    async fn delete_file(&self, key: &str) -> Result<(), StorageLayerError>;

    async fn get_file(&self, key: &str) -> Result<FileStream, StorageLayerError>;
}

/// Stream of bytes from a file
pub struct FileStream {
    pub stream: Pin<Box<dyn Stream<Item = std::io::Result<Bytes>> + Send>>,
}

impl Stream for FileStream {
    type Item = std::io::Result<Bytes>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.stream.as_mut().poll_next(cx)
    }
}

impl FileStream {
    /// Collect the stream to completion as a single [Bytes] buffer
    pub async fn collect_bytes(mut self) -> Result<Bytes, StorageLayerError> {
        let mut output = SegmentedBuf::new();

        while let Some(result) = self.next().await {
            let chunk = result.map_err(|error| {
                tracing::error!(?error, "failed to collect file stream bytes");
                StorageLayerError::CollectBytes
            })?;

            output.push(chunk);
        }

        Ok(output.copy_to_bytes(output.remaining()))
    }
}
