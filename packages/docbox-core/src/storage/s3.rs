use std::{error::Error, time::Duration};

use super::{FileStream, StorageLayer};
use crate::aws::S3Client;
use anyhow::Context;
use aws_config::SdkConfig;
use aws_sdk_s3::{
    config::Credentials,
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
use reqwest::StatusCode;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct S3StorageLayerFactoryConfig {
    pub endpoint: S3Endpoint,
}

impl S3StorageLayerFactoryConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let endpoint = S3Endpoint::from_env()?;

        Ok(Self { endpoint })
    }
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

impl S3Endpoint {
    pub fn from_env() -> anyhow::Result<Self> {
        match std::env::var("DOCBOX_S3_ENDPOINT") {
            // Using a custom S3 endpoint
            Ok(endpoint_url) => {
                let access_key_id = std::env::var("DOCBOX_S3_ACCESS_KEY_ID").context(
                    "cannot use DOCBOX_S3_ENDPOINT without specifying DOCBOX_S3_ACCESS_KEY_ID",
                )?;
                let access_key_secret = std::env::var("DOCBOX_S3_ACCESS_KEY_SECRET").context(
                    "cannot use DOCBOX_S3_ENDPOINT without specifying DOCBOX_S3_ACCESS_KEY_SECRET",
                )?;

                Ok(S3Endpoint::Custom {
                    endpoint: endpoint_url,
                    access_key_id,
                    access_key_secret,
                })
            }
            Err(_) => Ok(S3Endpoint::Aws),
        }
    }
}

#[derive(Clone)]
pub struct S3StorageLayerFactory {
    /// Client to access S3
    client: S3Client,
}

impl S3StorageLayerFactory {
    pub fn from_config(aws_config: &SdkConfig, config: S3StorageLayerFactoryConfig) -> Self {
        let client = match config.endpoint {
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

        Self { client }
    }

    pub fn create_storage_layer(&self, bucket_name: String) -> S3StorageLayer {
        S3StorageLayer {
            client: self.client.clone(),
            bucket_name,
        }
    }
}

#[derive(Clone)]
pub struct S3StorageLayer {
    /// Client to access S3
    client: S3Client,

    /// Name of the bucket to use
    bucket_name: String,
}

impl S3StorageLayer {
    pub fn new(client: S3Client, bucket_name: String) -> Self {
        Self {
            client,
            bucket_name,
        }
    }
}

impl StorageLayer for S3StorageLayer {
    async fn create_bucket(&self) -> anyhow::Result<()> {
        let bucket_region = self
            .client
            .config()
            .region()
            .context("AWS config missing AWS_REGION")?
            .to_string();

        let constraint = BucketLocationConstraint::from(bucket_region.as_str());

        let cfg = CreateBucketConfiguration::builder()
            .location_constraint(constraint)
            .build();

        if let Err(err) = self
            .client
            .create_bucket()
            .create_bucket_configuration(cfg)
            .bucket(&self.bucket_name)
            .send()
            .await
        {
            let already_exists = err
                .as_service_error()
                .is_some_and(|value| value.is_bucket_already_owned_by_you());

            // Bucket has already been created
            if already_exists {
                return Ok(());
            }

            tracing::error!(cause = ?err, "failed to create bucket");

            return Err(err.into());
        }

        Ok(())
    }

    async fn delete_bucket(&self) -> anyhow::Result<()> {
        self.client
            .delete_bucket()
            .bucket(&self.bucket_name)
            .send()
            .await
            .context("failed to delete bucket")?;

        Ok(())
    }

    async fn upload_file(
        &self,
        key: &str,
        content_type: String,
        body: Bytes,
    ) -> anyhow::Result<()> {
        self.client
            .put_object()
            .bucket(&self.bucket_name)
            .content_type(content_type)
            .key(key)
            .body(body.into())
            .send()
            .await
            .context("failed to store file in s3 bucket")?;

        Ok(())
    }

    async fn create_presigned(
        &self,
        key: &str,
        size: i64,
    ) -> anyhow::Result<(PresignedRequest, DateTime<Utc>)> {
        let expiry_time_minutes = 30;
        let expires_at = Utc::now()
            .checked_add_signed(TimeDelta::minutes(expiry_time_minutes))
            .context("expiry time exceeds unix limit")?;

        let result = self
            .client
            .put_object()
            .bucket(&self.bucket_name)
            .key(key)
            .content_length(size)
            .presigned(
                PresigningConfig::builder()
                    .expires_in(Duration::from_secs(60 * expiry_time_minutes as u64))
                    .build()?,
            )
            .await
            .context("failed to create presigned request")?;

        Ok((result, expires_at))
    }

    async fn add_bucket_notifications(&self, sqs_arn: &str) -> anyhow::Result<()> {
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
                            .build()?,
                    ]))
                    .build(),
            )
            .send()
            .await?;

        Ok(())
    }

    async fn add_bucket_cors(&self, origins: Vec<String>) -> anyhow::Result<()> {
        if let Err(cause) = self
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
                            .build()?,
                    )
                    .build()?,
            )
            .send()
            .await
        {
            // Handle "NotImplemented" errors (Local minio testing server does not have CORS support)
            if cause.raw_response().is_some_and(|response| {
                response.status().as_u16() == StatusCode::NOT_IMPLEMENTED.as_u16()
            }) {
                return Ok(());
            }

            return Err(cause.into());
        };

        Ok(())
    }

    async fn delete_file(&self, key: &str) -> anyhow::Result<()> {
        if let Err(cause) = self
            .client
            .delete_object()
            .bucket(&self.bucket_name)
            .key(key)
            .send()
            .await
        {
            // Handle keys that don't exist in the bucket
            // (This is not a failure and indicates the file is already deleted)
            if cause
                .as_service_error()
                .and_then(|err| err.source())
                .and_then(|source| source.downcast_ref::<aws_sdk_s3::Error>())
                .is_some_and(|err| matches!(err, aws_sdk_s3::Error::NoSuchKey(_)))
            {
                return Ok(());
            }

            return Err(cause.into());
        }

        Ok(())
    }

    async fn get_file(&self, key: &str) -> anyhow::Result<FileStream> {
        let object = self
            .client
            .get_object()
            .bucket(&self.bucket_name)
            .key(key)
            .send()
            .await?;

        let stream = FileStream {
            stream: Box::pin(AwsFileStream { inner: object.body }),
        };

        Ok(stream)
    }
}

pub struct AwsFileStream {
    inner: ByteStream,
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
