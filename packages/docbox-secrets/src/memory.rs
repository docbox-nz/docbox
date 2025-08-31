use tokio::sync::Mutex;

use super::{Secret, SecretManager};
use std::collections::HashMap;

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

    async fn create_secret(&self, name: &str, value: &str) -> anyhow::Result<()> {
        self.data
            .lock()
            .await
            .insert(name.to_string(), Secret::String(value.to_string()));
        Ok(())
    }
}
