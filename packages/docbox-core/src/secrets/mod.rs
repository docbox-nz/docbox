use async_trait::async_trait;
use aws::AwsSecretManager;
use aws_config::SdkConfig;
use docbox_database::{DbConnectErr, DbSecretManager, DbSecrets};
use memory::MemorySecretManager;
use serde::{Deserialize, de::DeserializeOwned};

use crate::{
    aws::SecretsManagerClient,
    secrets::{
        aws::AwsSecretManagerConfig,
        json::{JsonSecretManager, JsonSecretManagerConfig},
    },
};

pub mod aws;
pub mod json;
pub mod memory;

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum SecretsManagerConfig {
    /// In-memory secret manager
    Memory(AwsSecretManagerConfig),

    /// Encrypted JSON file secret manager
    Json(JsonSecretManagerConfig),

    /// AWS secret manager
    Aws,
}

impl SecretsManagerConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let variant = std::env::var("DOCBOX_SECRET_MANAGER").unwrap_or_else(|_| "aws".to_string());
        match variant.as_str() {
            "memory" => AwsSecretManagerConfig::from_env().map(Self::Memory),
            "json" => JsonSecretManagerConfig::from_env().map(Self::Json),
            _ => Ok(Self::Aws),
        }
    }
}

pub enum AppSecretManager {
    Aws(AwsSecretManager),
    Memory(MemorySecretManager),
    Json(JsonSecretManager),
}

impl AppSecretManager {
    /// Create the secret manager from the provided config
    pub fn from_config(aws_config: &SdkConfig, config: SecretsManagerConfig) -> Self {
        match config {
            SecretsManagerConfig::Memory(config) => {
                tracing::debug!("using in memory secret manager");
                AppSecretManager::Memory(MemorySecretManager::new(
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
                AppSecretManager::Json(JsonSecretManager::from_config(config))
            }
            SecretsManagerConfig::Aws => {
                tracing::debug!("using aws secret manager");
                let client = SecretsManagerClient::new(aws_config);
                AppSecretManager::Aws(AwsSecretManager::new(client))
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

    pub async fn create_secret(&self, name: &str, value: &str) -> anyhow::Result<()> {
        tracing::debug!(?name, "writing secret");
        match self {
            AppSecretManager::Aws(inner) => inner.create_secret(name, value).await,
            AppSecretManager::Memory(inner) => inner.create_secret(name, value).await,
            AppSecretManager::Json(inner) => inner.create_secret(name, value).await,
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

    async fn create_secret(&self, name: &str, value: &str) -> anyhow::Result<()>;
}

#[async_trait]
impl DbSecretManager for AppSecretManager {
    async fn get_secret(&self, name: &str) -> Result<Option<DbSecrets>, DbConnectErr> {
        self.parsed_secret(name)
            .await
            .map_err(|err| DbConnectErr::SecretsManager(err.into()))
    }
}
