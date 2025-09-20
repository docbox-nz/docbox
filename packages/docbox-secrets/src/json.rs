//! # JSON Secret Manager
//!
//! Encrypted local JSON based secrets manager, secrets are stored within a local
//! JSON file encrypted using [age](https://github.com/str4d/rage) encryption
//!
//! Intended for self-hosted environments where AWS secrets manager is not available
//!
//! ## Environment Variables
//!
//! * `DOCBOX_SECRET_MANAGER_KEY` - Specifies the encryption key to use
//! * `DOCBOX_SECRET_MANAGER_PATH` - Path to the encrypted JSON file

use crate::{Secret, SecretManagerError, SecretManagerImpl};
use age::secrecy::SecretString;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fmt::Debug, io, path::PathBuf};
use thiserror::Error;

#[derive(Clone, Deserialize, Serialize)]
pub struct JsonSecretManagerConfig {
    /// Encryption key to use
    pub key: String,

    /// Path to the encrypted JSON file
    pub path: PathBuf,
}

#[derive(Debug, Error)]
pub enum JsonSecretManagerConfigError {
    #[error("missing DOCBOX_SECRET_MANAGER_KEY secret key to access store")]
    MissingKey,
    #[error("missing DOCBOX_SECRET_MANAGER_PATH file path to access store")]
    MissingPath,
}

impl Debug for JsonSecretManagerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JsonSecretManagerConfig")
            .field("path", &self.path)
            .finish()
    }
}

impl JsonSecretManagerConfig {
    /// Load a config from environment variables
    pub fn from_env() -> Result<Self, JsonSecretManagerConfigError> {
        let key = std::env::var("DOCBOX_SECRET_MANAGER_KEY")
            .map_err(|_| JsonSecretManagerConfigError::MissingKey)?;
        let path = std::env::var("DOCBOX_SECRET_MANAGER_PATH")
            .map_err(|_| JsonSecretManagerConfigError::MissingPath)?;

        Ok(Self {
            key,
            path: PathBuf::from(&path),
        })
    }
}

// Local encrypted JSON based secret manager
pub struct JsonSecretManager {
    path: PathBuf,
    key: SecretString,
}

/// Temporary structure secrets are loaded into when loaded from a file
#[derive(Deserialize, Serialize)]
struct SecretFile {
    /// Secrets contained within the file as key-value pair
    secrets: HashMap<String, String>,
}

#[derive(Debug, Error)]
pub enum JsonSecretError {
    #[error("failed to read secrets")]
    ReadFile(io::Error),
    #[error("failed to write secrets")]
    WriteFile(io::Error),
    #[error("failed to decrypt secrets")]
    Decrypt(age::DecryptError),
    #[error("failed to encrypt secrets")]
    Encrypt(age::EncryptError),
    #[error("failed to deserialize secrets")]
    Deserialize(serde_json::Error),
    #[error("failed to serialize secrets")]
    Serialize(serde_json::Error),
}

impl JsonSecretManager {
    pub fn from_config(config: JsonSecretManagerConfig) -> Self {
        let key = SecretString::from(config.key);

        Self {
            path: config.path,
            key,
        }
    }

    async fn read_file(&self) -> Result<SecretFile, JsonSecretError> {
        let bytes = tokio::fs::read(&self.path).await.map_err(|error| {
            tracing::error!(?error, "failed to read secrets file");
            JsonSecretError::ReadFile(error)
        })?;

        let identity = age::scrypt::Identity::new(self.key.clone());
        let decrypted = age::decrypt(&identity, &bytes).map_err(|error| {
            tracing::error!(?error, "failed to decrypt secrets file");
            JsonSecretError::Decrypt(error)
        })?;

        let file = serde_json::from_slice(&decrypted).map_err(|error| {
            tracing::error!(?error, "failed to deserialize secrets file");
            JsonSecretError::Deserialize(error)
        })?;

        Ok(file)
    }

    async fn write_file(&self, file: SecretFile) -> Result<(), JsonSecretError> {
        let bytes = serde_json::to_string(&file).map_err(|error| {
            tracing::error!(?error, "failed to serialize secrets file");
            JsonSecretError::Serialize(error)
        })?;

        let recipient = age::scrypt::Recipient::new(self.key.clone());
        let encrypted = age::encrypt(&recipient, bytes.as_bytes()).map_err(|error| {
            tracing::error!(?error, "failed to encrypt secrets file");
            JsonSecretError::Encrypt(error)
        })?;

        tokio::fs::write(&self.path, encrypted)
            .await
            .map_err(|error| {
                tracing::error!(?error, "failed to write secrets file");
                JsonSecretError::WriteFile(error)
            })?;

        Ok(())
    }
}

impl SecretManagerImpl for JsonSecretManager {
    async fn get_secret(&self, name: &str) -> Result<Option<Secret>, SecretManagerError> {
        let file = match self.read_file().await {
            Ok(value) => value,
            Err(_) => return Ok(None),
        };

        let secret = file.secrets.get(name);
        Ok(secret.map(|value| Secret::String(value.clone())))
    }

    async fn set_secret(&self, name: &str, value: &str) -> Result<(), SecretManagerError> {
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

    async fn delete_secret(&self, name: &str) -> Result<(), SecretManagerError> {
        let mut secrets = if self.path.exists() {
            self.read_file().await?
        } else {
            SecretFile {
                secrets: Default::default(),
            }
        };

        secrets.secrets.remove(name);
        self.write_file(secrets).await?;
        Ok(())
    }
}
