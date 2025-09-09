use aws_sdk_secretsmanager::{
    error::SdkError,
    operation::{
        create_secret::CreateSecretError, get_secret_value::GetSecretValueError,
        update_secret::UpdateSecretError,
    },
};
use std::fmt::Debug;
use thiserror::Error;

use crate::SecretsManagerClient;

use super::{Secret, SecretManager};

pub struct AwsSecretManager {
    client: SecretsManagerClient,
}

impl AwsSecretManager {
    pub fn new(client: SecretsManagerClient) -> Self {
        Self { client }
    }
}

#[derive(Debug, Error)]
pub enum AwsSecretError {
    #[error("failed to get secret value: {0}")]
    GetSecretValue(SdkError<GetSecretValueError>),
    #[error("failed to create secret: {0}")]
    CreateSecret(SdkError<CreateSecretError>),
    #[error("failed to update secret: {0}")]
    UpdateSecret(SdkError<UpdateSecretError>),
}

impl SecretManager for AwsSecretManager {
    async fn get_secret(&self, name: &str) -> anyhow::Result<Option<super::Secret>> {
        let result = self
            .client
            .get_secret_value()
            .secret_id(name)
            .send()
            .await
            .map_err(AwsSecretError::GetSecretValue)?;

        if let Some(value) = result.secret_string {
            return Ok(Some(Secret::String(value)));
        }

        if let Some(value) = result.secret_binary {
            return Ok(Some(Secret::Binary(value.into_inner())));
        }

        Ok(None)
    }

    async fn create_secret(&self, name: &str, value: &str) -> anyhow::Result<()> {
        let err = match self
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
        if err
            .as_service_error()
            .is_some_and(|value| value.is_resource_exists_exception())
        {
            self.client
                .update_secret()
                .secret_string(value)
                .secret_id(name)
                .send()
                .await
                .map_err(AwsSecretError::UpdateSecret)?;

            return Ok(());
        }

        Err(AwsSecretError::CreateSecret(err).into())
    }
}
