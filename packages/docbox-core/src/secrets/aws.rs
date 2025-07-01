use std::{collections::HashMap, fmt::Debug};

use aws_sdk_secretsmanager::{
    error::SdkError,
    operation::{create_secret::CreateSecretError, get_secret_value::GetSecretValueError},
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::aws::SecretsManagerClient;

use super::{Secret, SecretManager};

#[derive(Clone, Deserialize, Serialize)]
pub struct AwsSecretManagerConfig {
    /// Collection of secrets to include
    #[serde(default)]
    pub secrets: HashMap<String, String>,
    /// Optional default secret
    #[serde(default)]
    pub default: Option<String>,
}

impl AwsSecretManagerConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let default = std::env::var("DOCBOX_SECRET_MANAGER_DEFAULT").ok();

        Ok(Self {
            default,
            secrets: Default::default(),
        })
    }
}

impl Debug for AwsSecretManagerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AwsSecretManagerConfig").finish()
    }
}

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
