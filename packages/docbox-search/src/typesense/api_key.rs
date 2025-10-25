use super::TypesenseSearchError;
use docbox_secrets::{Secret, SecretManager};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use tokio::sync::Mutex;

/// Provider for how an API key is sourced
pub enum TypesenseApiKeyProvider {
    ApiKey(TypesenseApiKey),
    Secret(TypesenseApiKeySecret),
}

impl ApiKeyProvider for TypesenseApiKeyProvider {
    async fn get_api_key(&self) -> Result<String, TypesenseSearchError> {
        match self {
            TypesenseApiKeyProvider::ApiKey(value) => value.get_api_key().await,
            TypesenseApiKeyProvider::Secret(value) => value.get_api_key().await,
        }
    }
}

/// Trait for something that can provide an API key for typesense
pub(crate) trait ApiKeyProvider {
    async fn get_api_key(&self) -> Result<String, TypesenseSearchError>;
}

/// API key from a secret manager that must be loaded at runtime
pub struct TypesenseApiKeySecret {
    /// Secret manager access
    secrets: SecretManager,

    /// Name of the secret the API key is within
    secret_name: String,

    /// Current loaded value of the secret
    secret_value: Mutex<Option<String>>,
}

impl TypesenseApiKeySecret {
    pub fn new(secrets: SecretManager, secret_name: String) -> Self {
        Self {
            secrets,
            secret_name,
            secret_value: Default::default(),
        }
    }
}

impl ApiKeyProvider for TypesenseApiKeySecret {
    async fn get_api_key(&self) -> Result<String, TypesenseSearchError> {
        let secret_value = &mut *self.secret_value.lock().await;
        if let Some(value) = secret_value.as_ref() {
            return Ok(value.clone());
        }

        match self.secrets.get_secret(&self.secret_name).await {
            Ok(Some(Secret::String(value))) => Ok(value),

            Ok(Some(Secret::Binary(_))) => {
                tracing::error!("expected string secret for typesense api key but got binary");
                Err(TypesenseSearchError::GetSecret)
            }

            Ok(None) => {
                tracing::error!(secret_name = ?self.secret_name, "secret not found");
                Err(TypesenseSearchError::GetSecret)
            }

            Err(error) => {
                tracing::error!(?error, "failed to get api key secret");
                Err(TypesenseSearchError::GetSecret)
            }
        }
    }
}

/// Known API key string value
#[derive(Clone, Deserialize, Serialize)]
#[serde(transparent)]
pub struct TypesenseApiKey(String);

impl TypesenseApiKey {
    pub fn new(value: String) -> Self {
        Self(value)
    }
}

// Prevent the API key itself from appearing in logs
impl Debug for TypesenseApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("< API KEY >")
    }
}

impl ApiKeyProvider for TypesenseApiKey {
    async fn get_api_key(&self) -> Result<String, TypesenseSearchError> {
        Ok(self.0.clone())
    }
}
