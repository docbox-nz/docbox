use async_trait::async_trait;
use aws::AwsSecretManager;
use aws_config::SdkConfig;
use docbox_database::{DbConnectErr, DbSecretManager, DbSecrets};
use memory::MemorySecretManager;
use serde::de::DeserializeOwned;

use crate::aws::SecretsManagerClient;

pub mod aws;
pub mod memory;

pub enum AppSecretManager {
    Aws(AwsSecretManager),
    Memory(MemorySecretManager),
}

impl AppSecretManager {
    pub fn from_env(aws_config: &SdkConfig) -> anyhow::Result<Self> {
        match std::env::var("DOCBOX_SECRET_MANAGER")
            .unwrap_or_else(|_| "opensearch".to_string())
            .as_str()
        {
            "memory" => {
                let default = std::env::var("DOCBOX_SECRET_MANAGER_DEFAULT")
                    .ok()
                    .map(Secret::String);

                Ok(AppSecretManager::Memory(MemorySecretManager::new(
                    Default::default(),
                    default,
                )))
            }
            _ => {
                let client = SecretsManagerClient::new(aws_config);
                Ok(AppSecretManager::Aws(AwsSecretManager::new(client)))
            }
        }
    }

    pub async fn get_secret(&self, name: &str) -> anyhow::Result<Option<Secret>> {
        match self {
            AppSecretManager::Aws(inner) => inner.get_secret(name).await,
            AppSecretManager::Memory(inner) => inner.get_secret(name).await,
        }
    }

    pub async fn create_secret(&self, name: &str, value: &str) -> anyhow::Result<()> {
        match self {
            AppSecretManager::Aws(inner) => inner.create_secret(name, value).await,
            AppSecretManager::Memory(inner) => inner.create_secret(name, value).await,
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
