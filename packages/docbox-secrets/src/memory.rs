//! # Memory secret manager
//!
//! In-memory secret manager for use within tests and local development
//! environments where a full secret manager is not needed
//!
//! ## Environment Variables
//!
//! * `DOCBOX_SECRET_MANAGER_MEMORY_DEFAULT` - Optional default secret value to provide when missing the secret
//! * `DOCBOX_SECRET_MANAGER_MEMORY_SECRETS` - JSON encoded hashmap of available secrets

use crate::{Secret, SecretManagerError, SecretManagerImpl, SetSecretOutcome};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, convert::Infallible, fmt::Debug, sync::Arc};
use thiserror::Error;
use tokio::sync::RwLock;

/// Secrets manager backed by memory
#[derive(Clone, Deserialize, Serialize)]
pub struct MemorySecretManagerConfig {
    /// Collection of secrets to include
    #[serde(default)]
    pub secrets: HashMap<String, String>,
    /// Optional default secret
    #[serde(default)]
    pub default: Option<String>,
}

/// Errors that could occur with the memory secrets manager config loaded
/// from the current environment
#[derive(Debug, Error)]
pub enum MemorySecretManagerConfigError {
    /// Failed to parse the secrets env variable
    #[error("failed to parse DOCBOX_SECRET_MANAGER_MEMORY_SECRETS")]
    ParseSecrets,
}

impl MemorySecretManagerConfig {
    /// Load a [MemorySecretManagerConfigError] from the current environment
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
#[derive(Default, Clone)]
pub struct MemorySecretManager {
    inner: Arc<RwLock<MemorySecretManagerInner>>,
}

#[derive(Default)]
struct MemorySecretManagerInner {
    data: HashMap<String, Secret>,
    default: Option<Secret>,
}

impl MemorySecretManager {
    /// Create a new memory secret manager from the provided values
    pub fn new(data: HashMap<String, Secret>, default: Option<Secret>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(MemorySecretManagerInner { data, default })),
        }
    }
}

/// Memory secret manager cannot fail
pub type MemorySecretError = Infallible;

impl SecretManagerImpl for MemorySecretManager {
    async fn get_secret(&self, name: &str) -> Result<Option<super::Secret>, SecretManagerError> {
        let inner = &*self.inner.read().await;

        if let Some(value) = inner.data.get(name) {
            return Ok(Some(value.clone()));
        }

        if let Some(value) = inner.default.as_ref() {
            return Ok(Some(value.clone()));
        }

        Ok(None)
    }

    async fn has_secret(&self, name: &str) -> Result<bool, SecretManagerError> {
        let inner = &*self.inner.read().await;
        Ok(inner.data.contains_key(name))
    }

    async fn set_secret(
        &self,
        name: &str,
        value: &str,
    ) -> Result<SetSecretOutcome, SecretManagerError> {
        let previous = self
            .inner
            .write()
            .await
            .data
            .insert(name.to_string(), Secret::String(value.to_string()));
        Ok(if previous.is_some() {
            SetSecretOutcome::Updated
        } else {
            SetSecretOutcome::Created
        })
    }

    async fn delete_secret(&self, name: &str) -> Result<(), SecretManagerError> {
        self.inner.write().await.data.remove(name);
        Ok(())
    }
}
