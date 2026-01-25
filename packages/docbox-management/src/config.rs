use aws_config::SdkConfig;
use docbox_core::{
    search::SearchIndexFactoryConfig,
    secrets::{
        SecretManager, SecretManagerError, SecretsManagerConfig,
        aws::{AwsSecretManagerConfig, AwsSecretsManagerConfigError},
    },
    storage::StorageLayerFactoryConfig,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Administrative database credentials configuration used for managing the database
#[derive(Clone, Deserialize, Serialize)]
pub struct AdminDatabaseConfiguration {
    /// Host of the database
    pub host: String,
    /// Port of the database
    pub port: u16,
    /// Setup user configuration if using an inline one
    pub setup_user: Option<AdminDatabaseSetupUserConfig>,
    /// Name of the setup user secret if using one
    pub setup_user_secret_name: Option<String>,
    /// Name of the root database secret
    pub root_secret_name: String,
}

/// Setup user configuration
#[derive(Clone, Deserialize, Serialize)]
pub struct AdminDatabaseSetupUserConfig {
    /// Username for the database account
    #[serde(alias = "user")]
    pub username: String,
    /// Password for the database account
    pub password: String,
}

/// Configuration for accessing the docbox API
#[derive(Clone, Deserialize, Serialize)]
pub struct ApiConfig {
    /// URL of the docbox server
    pub url: String,
    /// API key to access the server with
    pub api_key: Option<String>,
}

/// Configuration for operating on a docbox server
#[derive(Clone, Deserialize, Serialize)]
pub struct ServerConfigData {
    /// Config for accessing the docbox API
    pub api: ApiConfig,
    /// Database configuration
    pub database: AdminDatabaseConfiguration,
    /// Secret manager configuration
    pub secrets: SecretsManagerConfig,
    /// Search index configuration
    pub search: SearchIndexFactoryConfig,
    /// Storage backend configuration
    pub storage: StorageLayerFactoryConfig,
}

#[derive(Debug, Error)]
pub enum ServerConfigDataSecretError {
    #[error("failed to load secret manager from env: {0}")]
    SecretManager(AwsSecretsManagerConfigError),

    #[error("failed to load secret: {0}")]
    Secret(SecretManagerError),

    #[error("secret not found")]
    SecretNotFound,
}

/// Load a [ServerConfigData] from the AWS secret manager
pub async fn load_server_config_data_secret(
    aws_config: &SdkConfig,
    secret_name: &str,
) -> Result<ServerConfigData, ServerConfigDataSecretError> {
    let secrets = SecretManager::from_config(
        aws_config,
        SecretsManagerConfig::Aws(
            AwsSecretManagerConfig::from_env()
                .map_err(ServerConfigDataSecretError::SecretManager)?,
        ),
    );

    secrets
        .parsed_secret(secret_name)
        .await
        .map_err(ServerConfigDataSecretError::Secret)?
        .ok_or(ServerConfigDataSecretError::SecretNotFound)
}
