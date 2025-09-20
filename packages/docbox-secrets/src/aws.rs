//! # AWS secret manager
//!
//! Secret manager backed by [AWS secrets manager](https://docs.aws.amazon.com/secretsmanager/).
//! Inherits the loaded [SdkConfig] and all configuration provided to it.
//!
//! Intended for AWS hosted environments

use crate::{Secret, SecretManagerError, SecretManagerImpl};
use aws_config::SdkConfig;
use std::fmt::Debug;
use thiserror::Error;

pub type SecretsManagerClient = aws_sdk_secretsmanager::Client;

pub struct AwsSecretManager {
    client: SecretsManagerClient,
}

impl AwsSecretManager {
    pub fn from_sdk_config(aws_config: &SdkConfig) -> Self {
        let client = SecretsManagerClient::new(aws_config);
        Self::new(client)
    }

    pub fn new(client: SecretsManagerClient) -> Self {
        Self { client }
    }
}

#[derive(Debug, Error)]
pub enum AwsSecretError {
    #[error("failed to get secret value")]
    GetSecretValue,
    #[error("failed to create secret")]
    CreateSecret,
    #[error("failed to delete secret")]
    DeleteSecret,
    #[error("failed to update secret")]
    UpdateSecret,
}

impl SecretManagerImpl for AwsSecretManager {
    async fn get_secret(&self, name: &str) -> Result<Option<super::Secret>, SecretManagerError> {
        let result = self
            .client
            .get_secret_value()
            .secret_id(name)
            .send()
            .await
            .map_err(|error| {
                tracing::error!(?error, "failed to get secret value");
                AwsSecretError::GetSecretValue
            })?;

        if let Some(value) = result.secret_string {
            return Ok(Some(Secret::String(value)));
        }

        if let Some(value) = result.secret_binary {
            return Ok(Some(Secret::Binary(value.into_inner())));
        }

        Ok(None)
    }

    async fn set_secret(&self, name: &str, value: &str) -> Result<(), SecretManagerError> {
        let error = match self
            .client
            .create_secret()
            .secret_string(value)
            .name(name)
            .send()
            .await
        {
            Ok(_) => return Ok(()),
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
                    AwsSecretError::UpdateSecret
                })?;

            return Ok(());
        }

        tracing::error!(?error, "failed to create secret");
        Err(AwsSecretError::CreateSecret.into())
    }

    async fn delete_secret(&self, name: &str) -> Result<(), SecretManagerError> {
        let error = match self.client.delete_secret().secret_id(name).send().await {
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
        Err(AwsSecretError::DeleteSecret.into())
    }
}
