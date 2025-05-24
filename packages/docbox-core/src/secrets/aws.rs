use anyhow::{bail, Context};

use crate::aws::SecretsManagerClient;

use super::{Secret, SecretManager};

pub struct AwsSecretManager {
    client: SecretsManagerClient,
}

impl AwsSecretManager {
    pub fn new(client: SecretsManagerClient) -> Self {
        Self { client }
    }
}

impl SecretManager for AwsSecretManager {
    async fn get_secret(&self, name: &str) -> anyhow::Result<super::Secret> {
        let result = self
            .client
            .get_secret_value()
            .secret_id(name)
            .send()
            .await
            .with_context(|| format!("failed to get secret: {name}"))?;

        if let Some(value) = result.secret_string {
            return Ok(Secret::String(value));
        }

        if let Some(value) = result.secret_binary {
            return Ok(Secret::Binary(value.into_inner()));
        }

        bail!("secret has no value")
    }

    async fn create_secret(&self, name: &str, value: &str) -> anyhow::Result<()> {
        self.client
            .create_secret()
            .secret_string(value)
            .name(name)
            .send()
            .await?;

        Ok(())
    }
}
