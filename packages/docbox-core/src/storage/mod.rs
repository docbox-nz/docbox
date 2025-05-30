use anyhow::Context;
use aws_config::SdkConfig;
use aws_sdk_s3::{config::Credentials, presigning::PresignedRequest};
use bytes::{Buf, Bytes};
use bytes_utils::SegmentedBuf;
use chrono::{DateTime, Utc};
use docbox_database::models::tenant::Tenant;
use futures::{Stream, StreamExt};
use s3::{S3StorageLayer, S3StorageLayerFactory};
use serde::Deserialize;
use std::pin::Pin;

use crate::aws::S3Client;

pub mod s3;

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum StorageLayerFactoryConfig {
    S3 { endpoint: S3Endpoint },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum S3Endpoint {
    Aws,
    Custom {
        endpoint: String,
        access_key_id: String,
        access_key_secret: String,
    },
}

impl StorageLayerFactoryConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let endpoint = match std::env::var("DOCBOX_S3_ENDPOINT") {
            // Using a custom S3 endpoint
            Ok(endpoint_url) => {
                let access_key_id = std::env::var("DOCBOX_S3_ACCESS_KEY_ID").context(
                    "cannot use DOCBOX_S3_ENDPOINT without specifying DOCBOX_S3_ACCESS_KEY_ID",
                )?;
                let access_key_secret = std::env::var("DOCBOX_S3_ACCESS_KEY_SECRET").context(
                    "cannot use DOCBOX_S3_ENDPOINT without specifying DOCBOX_S3_ACCESS_KEY_SECRET",
                )?;

                S3Endpoint::Custom {
                    endpoint: endpoint_url,
                    access_key_id,
                    access_key_secret,
                }
            }
            Err(_) => S3Endpoint::Aws,
        };

        Ok(Self::S3 { endpoint })
    }
}

#[derive(Clone)]
pub enum StorageLayerFactory {
    S3(S3StorageLayerFactory),
}

impl StorageLayerFactory {
    pub fn from_config(aws_config: &SdkConfig, config: StorageLayerFactoryConfig) -> Self {
        match config {
            StorageLayerFactoryConfig::S3 { endpoint } => {
                let s3_client = match endpoint {
                    S3Endpoint::Aws => {
                        tracing::debug!("using aws s3 storage layer");
                        S3Client::new(aws_config)
                    }
                    S3Endpoint::Custom {
                        endpoint,
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
                        let config = aws_sdk_s3::config::Builder::from(aws_config)
                            .force_path_style(true)
                            .endpoint_url(endpoint)
                            .credentials_provider(credentials)
                            .build();
                        S3Client::from_conf(config)
                    }
                };

                let s3_storage_factory = S3StorageLayerFactory::new(s3_client);
                Self::S3(s3_storage_factory)
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
