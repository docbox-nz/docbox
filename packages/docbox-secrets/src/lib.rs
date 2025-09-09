//! # Secret manager
//!
//! Secret management abstraction
//!
//! ## Environment Variables
//!
//! * `DOCBOX_SECRET_MANAGER` - Which secret manager to use ("aws", "json", "memory")
//!
//! See individual secret manager module documentation for individual environment variables

use aws_config::SdkConfig;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

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

impl SecretsManagerConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let variant = std::env::var("DOCBOX_SECRET_MANAGER").unwrap_or_else(|_| "aws".to_string());
        match variant.as_str() {
            "memory" => memory::MemorySecretManagerConfig::from_env().map(Self::Memory),
            "json" => json::JsonSecretManagerConfig::from_env().map(Self::Json),
            _ => Ok(Self::Aws),
        }
    }
}

pub enum AppSecretManager {
    Aws(aws::AwsSecretManager),
    Memory(memory::MemorySecretManager),
    Json(json::JsonSecretManager),
}

impl AppSecretManager {
    /// Create the secret manager from the provided config
    pub fn from_config(aws_config: &SdkConfig, config: SecretsManagerConfig) -> Self {
        match config {
            SecretsManagerConfig::Memory(config) => {
                tracing::debug!("using in memory secret manager");
                AppSecretManager::Memory(memory::MemorySecretManager::new(
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
                AppSecretManager::Json(json::JsonSecretManager::from_config(config))
            }
            SecretsManagerConfig::Aws => {
                tracing::debug!("using aws secret manager");
                AppSecretManager::Aws(aws::AwsSecretManager::from_sdk_config(aws_config))
            }
        }
    }

    pub async fn get_secret(&self, name: &str) -> anyhow::Result<Option<Secret>> {
        tracing::debug!(?name, "reading secret");
        match self {
            AppSecretManager::Aws(inner) => inner.get_secret(name).await,
            AppSecretManager::Memory(inner) => inner.get_secret(name).await,
            AppSecretManager::Json(inner) => inner.get_secret(name).await,
        }
    }

    pub async fn set_secret(&self, name: &str, value: &str) -> anyhow::Result<()> {
        tracing::debug!(?name, "writing secret");
        match self {
            AppSecretManager::Aws(inner) => inner.set_secret(name, value).await,
            AppSecretManager::Memory(inner) => inner.set_secret(name, value).await,
            AppSecretManager::Json(inner) => inner.set_secret(name, value).await,
        }
    }

    pub async fn delete_secret(&self, name: &str) -> anyhow::Result<()> {
        tracing::debug!(?name, "deleting secret");
        match self {
            AppSecretManager::Aws(inner) => inner.delete_secret(name).await,
            AppSecretManager::Memory(inner) => inner.delete_secret(name).await,
            AppSecretManager::Json(inner) => inner.delete_secret(name).await,
        }
    }

    /// Obtains a secret parsing it as JSON of type [D]
    pub async fn parsed_secret<D: DeserializeOwned>(
        &self,
        name: &str,
    ) -> anyhow::Result<Option<D>> {
        let secret = match self.get_secret(name).await? {
            Some(value) => value,
            None => return Ok(None),
        };
        let value: D = match secret {
            Secret::String(value) => serde_json::from_str(&value)?,
            Secret::Binary(value) => serde_json::from_slice(value.as_ref())?,
        };
        Ok(Some(value))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Secret {
    String(String),
    Binary(Vec<u8>),
}

pub(crate) trait SecretManager: Send + Sync {
    async fn get_secret(&self, name: &str) -> anyhow::Result<Option<Secret>>;

    async fn set_secret(&self, name: &str, value: &str) -> anyhow::Result<()>;

    async fn delete_secret(&self, name: &str) -> anyhow::Result<()>;
}
