//! # Secret manager
//!
//! Secret management abstraction with multiple supported backends
//!
//! ## Environment Variables
//!
//! * `DOCBOX_SECRET_MANAGER` - Which secret manager to use ("aws", "json", "memory")
//!
//! See individual secret manager module documentation for individual environment variables

use aws_config::SdkConfig;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use thiserror::Error;

pub mod aws;
pub mod json;
pub mod memory;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum SecretsManagerConfig {
    /// In-memory secret manager
    Memory(memory::MemorySecretManagerConfig),

    /// Encrypted JSON file secret manager
    Json(json::JsonSecretManagerConfig),

    /// AWS secret manager
    Aws,
}

#[derive(Debug, Error)]
pub enum SecretsManagerConfigError {
    #[error(transparent)]
    Memory(memory::MemorySecretManagerConfigError),
    #[error(transparent)]
    Json(json::JsonSecretManagerConfigError),
}

impl SecretsManagerConfig {
    /// Get the current secret manager config from environment variables
    pub fn from_env() -> Result<Self, SecretsManagerConfigError> {
        let variant = std::env::var("DOCBOX_SECRET_MANAGER").unwrap_or_else(|_| "aws".to_string());
        match variant.as_str() {
            "memory" => memory::MemorySecretManagerConfig::from_env()
                .map(Self::Memory)
                .map_err(SecretsManagerConfigError::Memory),
            "json" => json::JsonSecretManagerConfig::from_env()
                .map(Self::Json)
                .map_err(SecretsManagerConfigError::Json),
            _ => Ok(Self::Aws),
        }
    }
}

/// Secret manager backed by some underlying
/// secret manager implementation
pub enum SecretManager {
    Aws(aws::AwsSecretManager),
    Memory(memory::MemorySecretManager),
    Json(json::JsonSecretManager),
}

impl SecretManager {
    /// Create the secret manager from the provided `config`
    ///
    /// The `aws_config` is required to provide aws specific settings when the AWS secret
    /// manager is used
    pub fn from_config(aws_config: &SdkConfig, config: SecretsManagerConfig) -> Self {
        match config {
            SecretsManagerConfig::Memory(config) => {
                tracing::debug!("using in memory secret manager");
                SecretManager::Memory(memory::MemorySecretManager::new(
                    config
                        .secrets
                        .into_iter()
                        .map(|(key, value)| (key, Secret::String(value)))
                        .collect(),
                    config.default.map(Secret::String),
                ))
            }

            SecretsManagerConfig::Json(config) => {
                tracing::debug!("using json secret manager");
                SecretManager::Json(json::JsonSecretManager::from_config(config))
            }
            SecretsManagerConfig::Aws => {
                tracing::debug!("using aws secret manager");
                SecretManager::Aws(aws::AwsSecretManager::from_sdk_config(aws_config))
            }
        }
    }

    /// Get a secret by `name`
    #[tracing::instrument(skip(self))]
    pub async fn get_secret(&self, name: &str) -> Result<Option<Secret>, SecretManagerError> {
        tracing::debug!(?name, "reading secret");
        match self {
            SecretManager::Aws(inner) => inner.get_secret(name).await,
            SecretManager::Memory(inner) => inner.get_secret(name).await,
            SecretManager::Json(inner) => inner.get_secret(name).await,
        }
    }

    /// Set the value of `name` secret to `value`
    ///
    /// Will create a new secret if the secret does not already exist
    #[tracing::instrument(skip(self))]
    pub async fn set_secret(&self, name: &str, value: &str) -> Result<(), SecretManagerError> {
        tracing::debug!(?name, "writing secret");
        match self {
            SecretManager::Aws(inner) => inner.set_secret(name, value).await,
            SecretManager::Memory(inner) => inner.set_secret(name, value).await,
            SecretManager::Json(inner) => inner.set_secret(name, value).await,
        }
    }

    /// Delete a secret by `name`
    #[tracing::instrument(skip(self))]
    pub async fn delete_secret(&self, name: &str) -> Result<(), SecretManagerError> {
        tracing::debug!(?name, "deleting secret");
        match self {
            SecretManager::Aws(inner) => inner.delete_secret(name).await,
            SecretManager::Memory(inner) => inner.delete_secret(name).await,
            SecretManager::Json(inner) => inner.delete_secret(name).await,
        }
    }

    /// Get a secret by `name` parsed as type [D] from JSON
    #[tracing::instrument(skip(self))]
    pub async fn parsed_secret<D: DeserializeOwned>(
        &self,
        name: &str,
    ) -> Result<Option<D>, SecretManagerError> {
        let secret = match self.get_secret(name).await? {
            Some(value) => value,
            None => return Ok(None),
        };

        let value: Result<D, serde_json::Error> = match secret {
            Secret::String(value) => serde_json::from_str(&value),
            Secret::Binary(value) => serde_json::from_slice(value.as_ref()),
        };

        let value = match value {
            Ok(value) => value,
            Err(error) => {
                tracing::error!(?error, "failed to parse JSON secret");
                return Err(SecretManagerError::ParseSecret);
            }
        };

        Ok(Some(value))
    }
}

#[derive(Debug, Error)]
pub enum SecretManagerError {
    #[error(transparent)]
    Memory(memory::MemorySecretError),

    #[error(transparent)]
    Json(Box<json::JsonSecretError>),

    #[error(transparent)]
    Aws(Box<aws::AwsSecretError>),

    #[error("failed to parse secret JSON")]
    ParseSecret,
}

impl From<json::JsonSecretError> for SecretManagerError {
    fn from(value: json::JsonSecretError) -> Self {
        Self::Json(Box::new(value))
    }
}

impl From<aws::AwsSecretError> for SecretManagerError {
    fn from(value: aws::AwsSecretError) -> Self {
        Self::Aws(Box::new(value))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Secret {
    /// Secret stored as a [String]
    String(String),

    /// Secret stored as bytes
    Binary(Vec<u8>),
}

/// Internal trait defining required async implementations for a secret manager
pub(crate) trait SecretManagerImpl: Send + Sync {
    async fn get_secret(&self, name: &str) -> Result<Option<Secret>, SecretManagerError>;

    async fn set_secret(&self, name: &str, value: &str) -> Result<(), SecretManagerError>;

    async fn delete_secret(&self, name: &str) -> Result<(), SecretManagerError>;
}
