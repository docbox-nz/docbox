#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! # Secret manager
//!
//! Secret management abstraction with multiple supported backends
//!
//! ## Environment Variables
//!
//! * `DOCBOX_SECRET_MANAGER` - Which secret manager to use ("aws", "json", "memory")
//!
//! See individual secret manager module documentation for individual environment variables
//!
//! - [aws]
//! - [json]
//! - [memory]

use aws_config::SdkConfig;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use thiserror::Error;

pub mod aws;
pub mod json;
pub mod memory;

/// Configuration for a secrets manager
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum SecretsManagerConfig {
    /// In-memory secret manager
    Memory(memory::MemorySecretManagerConfig),

    /// Encrypted JSON file secret manager
    Json(json::JsonSecretManagerConfig),

    /// AWS secret manager
    Aws(aws::AwsSecretManagerConfig),
}

/// Errors that could occur with a secrets manager config
#[derive(Debug, Error)]
pub enum SecretsManagerConfigError {
    /// Error from the memory secrets manager config
    #[error(transparent)]
    Memory(memory::MemorySecretManagerConfigError),

    /// Error from the JSON secrets manager config
    #[error(transparent)]
    Json(json::JsonSecretManagerConfigError),

    /// Error from the AWS secrets manager config
    #[error(transparent)]
    Aws(aws::AwsSecretsManagerConfigError),
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
            _ => aws::AwsSecretManagerConfig::from_env()
                .map(Self::Aws)
                .map_err(SecretsManagerConfigError::Aws),
        }
    }
}

/// Secret manager backed by some underlying secret manager implementation
#[derive(Clone)]
pub enum SecretManager {
    /// AWS backed secret manager
    Aws(aws::AwsSecretManager),

    /// In-memory secret manager
    Memory(memory::MemorySecretManager),

    /// Encrypted JSON backed secret manager
    Json(json::JsonSecretManager),
}

/// Outcome from setting a secret
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetSecretOutcome {
    /// Fresh secret was created
    Created,
    /// Secret with the same name was updated
    Updated,
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
            SecretsManagerConfig::Aws(config) => {
                tracing::debug!("using aws secret manager");
                SecretManager::Aws(aws::AwsSecretManager::from_config(aws_config, config))
            }
        }
    }

    /// Get a secret by `name`
    ///
    /// When using the memory secret manager this may return a default value, other secret
    /// managers will only return the actual secret
    #[tracing::instrument(skip(self))]
    pub async fn get_secret(&self, name: &str) -> Result<Option<Secret>, SecretManagerError> {
        tracing::debug!(?name, "reading secret");
        match self {
            SecretManager::Aws(inner) => inner.get_secret(name).await,
            SecretManager::Memory(inner) => inner.get_secret(name).await,
            SecretManager::Json(inner) => inner.get_secret(name).await,
        }
    }

    /// Check if a secret exists by `name`
    ///
    /// For the in-memory secret manager this will not return true unless the secret
    /// actually exists (Unlike [SecretManager::get_secret] which can return the default)
    #[tracing::instrument(skip(self))]
    pub async fn has_secret(&self, name: &str) -> Result<bool, SecretManagerError> {
        tracing::debug!(?name, "reading secret");
        match self {
            SecretManager::Aws(inner) => inner.has_secret(name).await,
            SecretManager::Memory(inner) => inner.has_secret(name).await,
            SecretManager::Json(inner) => inner.has_secret(name).await,
        }
    }

    /// Set the value of `name` secret to `value`
    ///
    /// Will create a new secret if the secret does not already exist
    #[tracing::instrument(skip(self))]
    pub async fn set_secret(
        &self,
        name: &str,
        value: &str,
    ) -> Result<SetSecretOutcome, SecretManagerError> {
        tracing::debug!(?name, "writing secret");
        match self {
            SecretManager::Aws(inner) => inner.set_secret(name, value).await,
            SecretManager::Memory(inner) => inner.set_secret(name, value).await,
            SecretManager::Json(inner) => inner.set_secret(name, value).await,
        }
    }

    /// Delete a secret by `name`
    #[tracing::instrument(skip(self))]
    pub async fn delete_secret(&self, name: &str, force: bool) -> Result<(), SecretManagerError> {
        tracing::debug!(?name, "deleting secret");
        match self {
            SecretManager::Aws(inner) => inner.delete_secret(name, force).await,
            SecretManager::Memory(inner) => inner.delete_secret(name, force).await,
            SecretManager::Json(inner) => inner.delete_secret(name, force).await,
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

/// Errors that could occur when using a secrets manager
#[derive(Debug, Error)]
pub enum SecretManagerError {
    /// In-memory secrets manager errors
    #[error(transparent)]
    Memory(memory::MemorySecretError),

    /// JSON secrets manager errors
    #[error(transparent)]
    Json(Box<json::JsonSecretError>),

    /// AWS secrets manager errors
    #[error(transparent)]
    Aws(Box<aws::AwsSecretError>),

    /// Error parsing a secret from JSON
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

/// Secret stored in a secrets manager
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

    async fn has_secret(&self, name: &str) -> Result<bool, SecretManagerError>;

    async fn set_secret(
        &self,
        name: &str,
        value: &str,
    ) -> Result<SetSecretOutcome, SecretManagerError>;

    async fn delete_secret(&self, name: &str, force: bool) -> Result<(), SecretManagerError>;
}
