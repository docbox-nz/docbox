//! # Memory secret manager
//!
//! In-memory secret manager for use within tests and local development
//! environments where a full secret manager is not needed
//!
//! ## Environment Variables
//!
//! * `DOCBOX_SECRET_MANAGER_MEMORY_DEFAULT` - Optional default secret value to provide when missing the secret
//! * `DOCBOX_SECRET_MANAGER_MEMORY_SECRETS` - JSON encoded hashmap of available secrets

use crate::{Secret, SecretManager, SecretManagerError};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, convert::Infallible, fmt::Debug};
use thiserror::Error;
use tokio::sync::Mutex;

#[derive(Clone, Deserialize, Serialize)]
pub struct MemorySecretManagerConfig {
    /// Collection of secrets to include
    #[serde(default)]
    pub secrets: HashMap<String, String>,
    /// Optional default secret
    #[serde(default)]
    pub default: Option<String>,
}

#[derive(Debug, Error)]
pub enum MemorySecretManagerConfigError {
    #[error("failed to parse DOCBOX_SECRET_MANAGER_MEMORY_SECRETS")]
    ParseSecrets,
}

impl MemorySecretManagerConfig {
    pub fn from_env() -> Result<Self, MemorySecretManagerConfigError> {
        let default = std::env::var("DOCBOX_SECRET_MANAGER_MEMORY_DEFAULT").ok();
        let secrets = match std::env::var("DOCBOX_SECRET_MANAGER_MEMORY_SECRETS") {
            Ok(secrets) => serde_json::from_str(&secrets).map_err(|error| {
                tracing::error!(?error, "failed to parse memory secrets");
                MemorySecretManagerConfigError::ParseSecrets
            })?,
            Err(_) => Default::default(),
        };

        Ok(Self { default, secrets })
    }
}

impl Debug for MemorySecretManagerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemorySecretManagerConfig").finish()
    }
}

/// In-memory secret manager
#[derive(Default)]
pub struct MemorySecretManager {
    data: Mutex<HashMap<String, Secret>>,
    default: Option<Secret>,
}

impl MemorySecretManager {
    pub fn new(data: HashMap<String, Secret>, default: Option<Secret>) -> Self {
        Self {
            data: Mutex::new(data),
            default,
        }
    }
}

pub type MemorySecretError = Infallible;

impl SecretManager for MemorySecretManager {
    async fn get_secret(&self, name: &str) -> Result<Option<super::Secret>, SecretManagerError> {
        if let Some(value) = self.data.lock().await.get(name) {
            return Ok(Some(value.clone()));
        }

        if let Some(value) = self.default.as_ref() {
            return Ok(Some(value.clone()));
        }

        Ok(None)
    }

    async fn set_secret(&self, name: &str, value: &str) -> Result<(), SecretManagerError> {
        self.data
            .lock()
            .await
            .insert(name.to_string(), Secret::String(value.to_string()));
        Ok(())
    }

    async fn delete_secret(&self, name: &str) -> Result<(), SecretManagerError> {
        self.data.lock().await.remove(name);
        Ok(())
    }
}
