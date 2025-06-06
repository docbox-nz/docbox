use std::{collections::HashMap, fmt::Debug, path::PathBuf, str::FromStr};

use age::secrecy::SecretString;
use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::secrets::SecretManager;

use super::Secret;

#[derive(Clone, Deserialize)]
pub struct JsonSecretManagerConfig {
    path: PathBuf,
    key: String,
}

impl Debug for JsonSecretManagerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JsonSecretManagerConfig")
            .field("path", &self.path)
            .finish()
    }
}

impl JsonSecretManagerConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let key = std::env::var("DOCBOX_SECRET_MANAGER_KEY")
            .context("missing DOCBOX_SECRET_MANAGER_KEY secret key to access store")?;
        let path = std::env::var("DOCBOX_SECRET_MANAGER_PATH")
            .context("missing DOCBOX_SECRET_MANAGER_PATH file path to access store")?;
        Ok(Self {
            key,
            path: PathBuf::from_str(&path)?,
        })
    }
}

#[derive(Deserialize, Serialize)]
struct SecretFile {
    secrets: HashMap<String, String>,
}

// Local encrypted JSON based secret manager
pub struct JsonSecretManager {
    path: PathBuf,
    key: SecretString,
}

impl JsonSecretManager {
    pub fn from_config(config: JsonSecretManagerConfig) -> Self {
        let key = SecretString::from(config.key);

        Self {
            path: config.path,
            key,
        }
    }

    async fn read_file(&self) -> anyhow::Result<SecretFile> {
        let bytes = tokio::fs::read(&self.path).await?;
        let identity = age::scrypt::Identity::new(self.key.clone());
        let decrypted = age::decrypt(&identity, &bytes)?;
        let file = serde_json::from_slice(&decrypted)?;
        Ok(file)
    }

    async fn write_file(&self, file: SecretFile) -> anyhow::Result<()> {
        let bytes = serde_json::to_string(&file)?;
        let recipient = age::scrypt::Recipient::new(self.key.clone());
        let encrypted = age::encrypt(&recipient, bytes.as_bytes())?;
        tokio::fs::write(&self.path, encrypted).await?;
        Ok(())
    }
}

impl SecretManager for JsonSecretManager {
    async fn get_secret(&self, name: &str) -> anyhow::Result<Option<Secret>> {
        let file = match self.read_file().await {
            Ok(value) => value,
            Err(_) => return Ok(None),
        };

        let secret = file.secrets.get(name);
        Ok(secret.map(|value| Secret::String(value.clone())))
    }

    async fn create_secret(&self, name: &str, value: &str) -> anyhow::Result<()> {
        let mut secrets = if self.path.exists() {
            self.read_file().await?
        } else {
            SecretFile {
                secrets: Default::default(),
            }
        };
        secrets.secrets.insert(name.to_string(), value.to_string());
        self.write_file(secrets).await?;
        Ok(())
    }
}
