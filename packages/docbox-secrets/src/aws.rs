//! # AWS secret manager
//!
//! Secret manager backed by [AWS secrets manager](https://docs.aws.amazon.com/secretsmanager/).
//! Inherits the loaded [SdkConfig] and all configuration provided to it.
//!
//! Intended for AWS hosted environments

use crate::{Secret, SecretManagerError, SecretManagerImpl, SetSecretOutcome};
use aws_config::SdkConfig;
use aws_sdk_secretsmanager::{
    config::{Credentials, SharedCredentialsProvider},
    error::SdkError,
    operation::{
        create_secret::CreateSecretError, delete_secret::DeleteSecretError,
        get_secret_value::GetSecretValueError, update_secret::UpdateSecretError,
    },
};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use thiserror::Error;

type SecretsManagerClient = aws_sdk_secretsmanager::Client;

/// Config for the JSON secret manager
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct AwsSecretManagerConfig {
    /// Endpoint to use for requests
    pub endpoint: AwsSecretsEndpoint,
}

impl AwsSecretManagerConfig {
    /// Load a [AwsSecretManagerConfig] from the current environment
    pub fn from_env() -> Result<Self, AwsSecretsManagerConfigError> {
        let endpoint = AwsSecretsEndpoint::from_env()?;
        Ok(Self { endpoint })
    }
}

/// AWS secrets manager backed secrets
#[derive(Clone)]
pub struct AwsSecretManager {
    client: SecretsManagerClient,
}

/// Endpoint to use for secrets manager operations
#[derive(Default, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AwsSecretsEndpoint {
    /// AWS default endpoint
    #[default]
    Aws,
    /// Custom endpoint (Loker or other compatible)
    Custom {
        /// Endpoint URL
        endpoint: String,
        /// Access key ID to use
        access_key_id: String,
        /// Access key secret to use
        access_key_secret: String,
    },
}

/// Errors that could occur when loading the AWS configuration
#[derive(Debug, Error)]
pub enum AwsSecretsManagerConfigError {
    /// Using a custom endpoint but didn't specify the access key ID
    #[error("cannot use DOCBOX_SECRETS_ACCESS_KEY_ID without specifying DOCBOX_S3_ACCESS_KEY_ID")]
    MissingAccessKeyId,

    /// Using a custom endpoint but didn't specify the access key secret
    #[error("cannot use DOCBOX_S3_ENDPOINT without specifying DOCBOX_S3_ACCESS_KEY_SECRET")]
    MissingAccessKeySecret,
}

impl Debug for AwsSecretsEndpoint {
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

impl AwsSecretsEndpoint {
    /// Load a [SecretsEndpoint] from the current environment
    pub fn from_env() -> Result<Self, AwsSecretsManagerConfigError> {
        match std::env::var("DOCBOX_SECRETS_ENDPOINT") {
            // Using a custom secrets endpoint
            Ok(endpoint_url) => {
                let access_key_id = std::env::var("DOCBOX_SECRETS_ACCESS_KEY_ID")
                    .map_err(|_| AwsSecretsManagerConfigError::MissingAccessKeyId)?;
                let access_key_secret = std::env::var("DOCBOX_SECRETS_ACCESS_KEY_SECRET")
                    .map_err(|_| AwsSecretsManagerConfigError::MissingAccessKeySecret)?;

                Ok(AwsSecretsEndpoint::Custom {
                    endpoint: endpoint_url,
                    access_key_id,
                    access_key_secret,
                })
            }
            Err(_) => Ok(AwsSecretsEndpoint::Aws),
        }
    }
}

impl AwsSecretManager {
    /// Create a [AwsSecretManager] from a [SdkConfig]
    pub fn from_config(aws_config: &SdkConfig, config: AwsSecretManagerConfig) -> Self {
        let client = match config.endpoint {
            AwsSecretsEndpoint::Aws => SecretsManagerClient::new(aws_config),
            AwsSecretsEndpoint::Custom {
                endpoint,
                access_key_id,
                access_key_secret,
            } => {
                // Apply custom credentials and endpoint
                let credentials =
                    Credentials::new(access_key_id, access_key_secret, None, None, "docbox");
                let aws_config = aws_config
                    .to_builder()
                    .endpoint_url(endpoint)
                    .credentials_provider(SharedCredentialsProvider::new(credentials))
                    .build();
                SecretsManagerClient::new(&aws_config)
            }
        };

        Self::new(client)
    }

    /// Create a [AwsSecretManager] from a [SecretsManagerClient]
    pub fn new(client: SecretsManagerClient) -> Self {
        Self { client }
    }
}

/// Errors that could occur when working with AWS secret manager
#[derive(Debug, Error)]
pub enum AwsSecretError {
    /// Failed to get a secret value
    #[error("failed to get secret value")]
    GetSecretValue(SdkError<GetSecretValueError>),

    /// Failed to create a secret
    #[error("failed to create secret")]
    CreateSecret(SdkError<CreateSecretError>),

    /// Failed to delete a secret
    #[error("failed to delete secret")]
    DeleteSecret(SdkError<DeleteSecretError>),

    /// Failed to update a secret
    #[error("failed to update secret")]
    UpdateSecret(SdkError<UpdateSecretError>),
}

impl SecretManagerImpl for AwsSecretManager {
    async fn get_secret(&self, name: &str) -> Result<Option<super::Secret>, SecretManagerError> {
        let result = match self.client.get_secret_value().secret_id(name).send().await {
            Ok(value) => value,
            Err(error) => {
                if error
                    .as_service_error()
                    .is_some_and(|value| value.is_resource_not_found_exception())
                {
                    return Ok(None);
                }

                tracing::error!(?error, "failed to get secret value");
                return Err(AwsSecretError::GetSecretValue(error).into());
            }
        };

        if let Some(value) = result.secret_string {
            return Ok(Some(Secret::String(value)));
        }

        if let Some(value) = result.secret_binary {
            return Ok(Some(Secret::Binary(value.into_inner())));
        }

        Ok(None)
    }

    async fn has_secret(&self, name: &str) -> Result<bool, SecretManagerError> {
        self.get_secret(name).await.map(|value| value.is_some())
    }

    async fn set_secret(
        &self,
        name: &str,
        value: &str,
    ) -> Result<SetSecretOutcome, SecretManagerError> {
        let error = match self
            .client
            .create_secret()
            .secret_string(value)
            .name(name)
            .send()
            .await
        {
            Ok(_) => return Ok(SetSecretOutcome::Created),
            Err(err) => err,
        };

        // Handle secret already existing
        if error
            .as_service_error()
            .is_some_and(|value| value.is_resource_exists_exception())
        {
            tracing::debug!("secret already exists, updating secret");

            self.client
                .update_secret()
                .secret_string(value)
                .secret_id(name)
                .send()
                .await
                .map_err(|error| {
                    tracing::error!(?error, "failed to update secret");
                    AwsSecretError::UpdateSecret(error)
                })?;

            return Ok(SetSecretOutcome::Updated);
        }

        tracing::error!(?error, "failed to create secret");
        Err(AwsSecretError::CreateSecret(error).into())
    }

    async fn delete_secret(&self, name: &str, force: bool) -> Result<(), SecretManagerError> {
        let error = match self
            .client
            .delete_secret()
            .secret_id(name)
            .force_delete_without_recovery(force)
            .send()
            .await
        {
            Ok(_) => return Ok(()),
            Err(error) => error,
        };

        // Handle secret doesn't exist
        if error
            .as_service_error()
            .is_some_and(|value| value.is_resource_not_found_exception())
        {
            tracing::debug!("secret does not exist");
            return Ok(());
        }

        tracing::error!(?error, "failed to create secret");
        Err(AwsSecretError::DeleteSecret(error).into())
    }
}
