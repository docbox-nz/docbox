use aws_sdk_secretsmanager::{
    error::SdkError,
    operation::{create_secret::CreateSecretError, get_secret_value::GetSecretValueError},
};
use thiserror::Error;

use crate::aws::SecretsManagerClient;

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
    #[error(transparent)]
    GetSecretValue(SdkError<GetSecretValueError>),
    #[error(transparent)]
    CreateSecret(SdkError<CreateSecretError>),
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
        self.client
            .create_secret()
            .secret_string(value)
            .name(name)
            .send()
            .await
            .map_err(AwsSecretError::CreateSecret)?;

        Ok(())
    }
}
