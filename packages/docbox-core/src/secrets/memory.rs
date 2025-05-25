use super::{Secret, SecretManager};
use std::collections::HashMap;

/// In memory secret manager
pub struct MemorySecretManager {
    data: HashMap<String, Secret>,
    default: Option<Secret>,
}

impl MemorySecretManager {
    pub fn new(data: HashMap<String, Secret>, default: Option<Secret>) -> Self {
        Self { data, default }
    }
}

impl SecretManager for MemorySecretManager {
    async fn get_secret(&self, name: &str) -> anyhow::Result<Option<super::Secret>> {
        if let Some(value) = self.data.get(name) {
            return Ok(Some(value.clone()));
        }

        if let Some(value) = self.default.as_ref() {
            return Ok(Some(value.clone()));
        }

        Ok(None)
    }

    async fn create_secret(&self, _name: &str, _value: &str) -> anyhow::Result<()> {
        Ok(())
    }
}
