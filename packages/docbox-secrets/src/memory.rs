//! # Memory secret manager
//!
//! In-memory secret manager for use within tests and local development
//! environments where a full secret manager is not needed
//!
//! ## Environment Variables
//!
//! * `DOCBOX_SECRET_MANAGER_MEMORY_DEFAULT` - Optional default secret value to provide when missing the secret
//! * `DOCBOX_SECRET_MANAGER_MEMORY_SECRETS` - JSON encoded hashmap of available secrets

use crate::{Secret, SecretManager};
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fmt::Debug};
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

impl MemorySecretManagerConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let default = std::env::var("DOCBOX_SECRET_MANAGER_MEMORY_DEFAULT").ok();
        let secrets = match std::env::var("DOCBOX_SECRET_MANAGER_MEMORY_SECRETS") {
            Ok(secrets) => serde_json::from_str(&secrets)
                .context("failed to parse DOCBOX_SECRET_MANAGER_MEMORY_SECRETS")?,
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

/// In memory secret manager
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

impl SecretManager for MemorySecretManager {
    async fn get_secret(&self, name: &str) -> anyhow::Result<Option<super::Secret>> {
        if let Some(value) = self.data.lock().await.get(name) {
            return Ok(Some(value.clone()));
        }

        if let Some(value) = self.default.as_ref() {
            return Ok(Some(value.clone()));
        }

        Ok(None)
    }

    async fn set_secret(&self, name: &str, value: &str) -> anyhow::Result<()> {
        self.data
            .lock()
            .await
            .insert(name.to_string(), Secret::String(value.to_string()));
        Ok(())
    }

    async fn delete_secret(&self, name: &str) -> anyhow::Result<()> {
        self.data.lock().await.remove(name);
        Ok(())
    }
}
