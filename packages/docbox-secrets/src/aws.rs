//! # AWS secret manager
//!
//! Secret manager backed by [AWS secrets manager](https://docs.aws.amazon.com/secretsmanager/).
//! Inherits the loaded [SdkConfig] and all configuration provided to it.
//!
//! Intended for AWS hosted environments

use crate::{Secret, SecretManagerError, SecretManagerImpl, SetSecretOutcome};
use aws_config::SdkConfig;
use aws_sdk_secretsmanager::{
    error::SdkError,
    operation::{
        create_secret::CreateSecretError, delete_secret::DeleteSecretError,
        get_secret_value::GetSecretValueError, update_secret::UpdateSecretError,
    },
};
use std::fmt::Debug;
use thiserror::Error;

type SecretsManagerClient = aws_sdk_secretsmanager::Client;

/// AWS secrets manager backed secrets
#[derive(Clone)]
pub struct AwsSecretManager {
    client: SecretsManagerClient,
}

impl AwsSecretManager {
    /// Create a [AwsSecretManager] from a [SdkConfig]
    pub fn from_sdk_config(aws_config: &SdkConfig) -> Self {
        let client = SecretsManagerClient::new(aws_config);
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
        let result = self
            .client
            .get_secret_value()
            .secret_id(name)
            .send()
            .await
            .map_err(|error| {
                tracing::error!(?error, "failed to get secret value");
                AwsSecretError::GetSecretValue(error)
            })?;

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
