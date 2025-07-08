use aws_config::SdkConfig;
use aws_sdk_s3::presigning::PresignedRequest;
use bytes::{Buf, Bytes};
use bytes_utils::SegmentedBuf;
use chrono::{DateTime, Utc};
use docbox_database::models::tenant::Tenant;
use futures::{Stream, StreamExt};
use s3::{S3StorageLayer, S3StorageLayerFactory};
use serde::{Deserialize, Serialize};
use std::{pin::Pin, time::Duration};

use crate::storage::s3::S3StorageLayerFactoryConfig;

pub mod s3;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum StorageLayerFactoryConfig {
    S3(S3StorageLayerFactoryConfig),
}

impl StorageLayerFactoryConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        S3StorageLayerFactoryConfig::from_env().map(Self::S3)
    }
}

#[derive(Clone)]
pub enum StorageLayerFactory {
    S3(S3StorageLayerFactory),
}

impl StorageLayerFactory {
    pub fn from_config(aws_config: &SdkConfig, config: StorageLayerFactoryConfig) -> Self {
        match config {
            StorageLayerFactoryConfig::S3(config) => {
                Self::S3(S3StorageLayerFactory::from_config(aws_config, config))
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
    S3(S3StorageLayer),
}

impl TenantStorageLayer {
    /// Creates the tenant S3 bucket
    pub async fn create_bucket(&self) -> anyhow::Result<()> {
        match self {
            TenantStorageLayer::S3(layer) => layer.create_bucket().await,
        }
    }

    /// Deletes the tenant S3 bucket
    pub async fn delete_bucket(&self) -> anyhow::Result<()> {
        match self {
            TenantStorageLayer::S3(layer) => layer.delete_bucket().await,
        }
    }

    /// Create a presigned file upload URL
    pub async fn create_presigned(
        &self,
        key: &str,
        size: i64,
    ) -> anyhow::Result<(PresignedRequest, DateTime<Utc>)> {
        match self {
            TenantStorageLayer::S3(layer) => layer.create_presigned(key, size).await,
        }
    }

    /// Create a presigned file download URL
    pub async fn create_presigned_download(
        &self,
        key: &str,
        expires_in: Duration,
    ) -> anyhow::Result<(PresignedRequest, DateTime<Utc>)> {
        match self {
            TenantStorageLayer::S3(layer) => layer.create_presigned_download(key, expires_in).await,
        }
    }

    /// Uploads a file to the S3 bucket for the tenant
    pub async fn upload_file(
        &self,
        key: &str,
        content_type: String,
        body: Bytes,
    ) -> anyhow::Result<()> {
        match self {
            TenantStorageLayer::S3(layer) => layer.upload_file(key, content_type, body).await,
        }
    }

    /// Add the SNS notification to a bucket
    pub async fn add_bucket_notifications(&self, sns_arn: &str) -> anyhow::Result<()> {
        match self {
            TenantStorageLayer::S3(layer) => layer.add_bucket_notifications(sns_arn).await,
        }
    }

    /// Applies CORS rules for a bucket
    pub async fn add_bucket_cors(&self, origins: Vec<String>) -> anyhow::Result<()> {
        match self {
            TenantStorageLayer::S3(layer) => layer.add_bucket_cors(origins).await,
        }
    }

    /// Deletes the S3 file
    pub async fn delete_file(&self, key: &str) -> anyhow::Result<()> {
        match self {
            TenantStorageLayer::S3(layer) => layer.delete_file(key).await,
        }
    }

    /// Gets a byte stream for a file from S3
    pub async fn get_file(&self, key: &str) -> anyhow::Result<FileStream> {
        match self {
            TenantStorageLayer::S3(layer) => layer.get_file(key).await,
        }
    }
}

pub(crate) trait StorageLayer {
    /// Creates the tenant S3 bucket
    async fn create_bucket(&self) -> anyhow::Result<()>;

    /// Deletes the tenant S3 bucket
    async fn delete_bucket(&self) -> anyhow::Result<()>;

    /// Create a presigned file upload URL
    async fn create_presigned(
        &self,
        key: &str,
        size: i64,
    ) -> anyhow::Result<(PresignedRequest, DateTime<Utc>)>;

    /// Create a presigned file download URL
    async fn create_presigned_download(
        &self,
        key: &str,
        expires_in: Duration,
    ) -> anyhow::Result<(PresignedRequest, DateTime<Utc>)>;

    /// Uploads a file to the S3 bucket for the tenant
    async fn upload_file(&self, key: &str, content_type: String, body: Bytes)
    -> anyhow::Result<()>;

    /// Add the SNS notification to a bucket
    async fn add_bucket_notifications(&self, sns_arn: &str) -> anyhow::Result<()>;

    /// Applies CORS rules for a bucket
    async fn add_bucket_cors(&self, origins: Vec<String>) -> anyhow::Result<()>;

    /// Deletes the S3 file
    async fn delete_file(&self, key: &str) -> anyhow::Result<()>;

    /// Gets a byte stream for a file from S3
    async fn get_file(&self, key: &str) -> anyhow::Result<FileStream>;
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
        // Pin projection to the underlying stream
        let stream = unsafe { self.as_mut().map_unchecked_mut(|s| &mut s.stream) };
        stream.poll_next(cx)
    }
}

impl FileStream {
    /// Collect the stream to completion as a single [Bytes] buffer
    pub async fn collect_bytes(mut self) -> anyhow::Result<Bytes> {
        let mut output = SegmentedBuf::new();

        while let Some(result) = self.next().await {
            let chunk = result?;
            output.push(chunk);
        }

        Ok(output.copy_to_bytes(output.remaining()))
    }
}
