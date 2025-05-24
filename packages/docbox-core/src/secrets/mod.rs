use async_trait::async_trait;
use aws::AwsSecretManager;
use docbox_database::{DbSecretManager, DbSecrets};
use memory::MemorySecretManager;
use serde::de::DeserializeOwned;

pub mod aws;
pub mod memory;

pub enum AppSecretManager {
    Aws(AwsSecretManager),
    Memory(MemorySecretManager),
}

impl AppSecretManager {
    pub async fn get_secret(&self, name: &str) -> anyhow::Result<Secret> {
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
    pub async fn parsed_secret<D: DeserializeOwned>(&self, name: &str) -> anyhow::Result<D> {
        let secret = self.get_secret(name).await?;
        let value: D = match secret {
            Secret::String(value) => serde_json::from_str(&value)?,
            Secret::Binary(value) => serde_json::from_slice(value.as_ref())?,
        };
        Ok(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Secret {
    String(String),
    Binary(Vec<u8>),
}

pub(crate) trait SecretManager: Send + Sync {
    async fn get_secret(&self, name: &str) -> anyhow::Result<Secret>;

    async fn create_secret(&self, name: &str, value: &str) -> anyhow::Result<()>;
}

#[async_trait]
impl DbSecretManager for AppSecretManager {
    async fn get_secret(&self, name: &str) -> anyhow::Result<DbSecrets> {
        self.parsed_secret(name).await
    }
}
