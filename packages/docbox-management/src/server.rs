use crate::{
    config::{AdminDatabaseSetupUserConfig, ServerConfigData, ServerConfigDataSecretError},
    database::ServerDatabaseProvider,
};
use aws_config::SdkConfig;
use docbox_core::{
    aws::SqsClient,
    database::{DatabasePoolCache, DatabasePoolCacheConfig},
    events::{EventPublisherFactory, sqs::SqsEventPublisherFactory},
    search::{SearchIndexFactory, SearchIndexFactoryError},
    secrets::{SecretManager, SecretManagerError},
    storage::StorageLayerFactory,
};
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LoadManagedServerError {
    #[error("failed to load server config secret: {0}")]
    SecretManager(#[from] SecretManagerError),

    #[error("failed to load server config secret: {0}")]
    LoadSecret(#[from] ServerConfigDataSecretError),

    #[error("server config database setup user secret not found")]
    MissingDatabaseSecret,

    #[error("must provided either setup_user or setup_user_secret_name in database config")]
    MissingSetupUser,

    #[error("failed to create search index factory: {0}")]
    CreateSearchFactory(#[from] SearchIndexFactoryError),
}

/// Loaded server with all dependencies required to perform
/// management actions on the server
pub struct ManagedServer {
    // Database provider for management database access
    pub db_provider: ServerDatabaseProvider,
    // Database access
    pub db_cache: Arc<DatabasePoolCache>,
    /// Secrets manager access
    pub secrets: SecretManager,
    /// Search access
    pub search: SearchIndexFactory,
    /// Storage access
    pub storage: StorageLayerFactory,
    /// Events access
    pub events: EventPublisherFactory,
}

/// Load a managed server to perform db actions
pub async fn load_managed_server(
    aws_config: &SdkConfig,
    config: &ServerConfigData,
) -> Result<ManagedServer, LoadManagedServerError> {
    // Setup server secret manager
    let secrets = SecretManager::from_config(aws_config, config.secrets.clone());

    // Setup database cache / connector
    let db_cache = Arc::new(DatabasePoolCache::from_config(
        DatabasePoolCacheConfig {
            host: config.database.host.clone(),
            port: config.database.port,
            root_secret_name: config.database.root_secret_name.clone(),
            ..Default::default()
        },
        secrets.clone(),
    ));

    // Setup search factory
    let search = SearchIndexFactory::from_config(
        aws_config,
        secrets.clone(),
        db_cache.clone(),
        config.search.clone(),
    )?;

    // Setup storage factory
    let storage = StorageLayerFactory::from_config(aws_config, config.storage.clone());

    let db_provider = match (
        config.database.setup_user.as_ref(),
        config.database.setup_user_secret_name.as_deref(),
    ) {
        (Some(setup_user), _) => ServerDatabaseProvider {
            config: config.database.clone(),
            username: setup_user.username.clone(),
            password: setup_user.password.clone(),
        },
        (_, Some(setup_user_secret_name)) => {
            let secret: AdminDatabaseSetupUserConfig = secrets
                .parsed_secret(setup_user_secret_name)
                .await?
                .ok_or(LoadManagedServerError::MissingDatabaseSecret)?;

            tracing::debug!("loaded database secrets from secret manager");

            ServerDatabaseProvider {
                config: config.database.clone(),
                username: secret.username.clone(),
                password: secret.password.clone(),
            }
        }
        (None, None) => {
            return Err(LoadManagedServerError::MissingSetupUser);
        }
    };

    // Create the SQS client
    // Warning: Will panic if the configuration provided is invalid
    let sqs_client = SqsClient::new(aws_config);

    // Setup event publisher factories
    let sqs_publisher_factory = SqsEventPublisherFactory::new(sqs_client.clone());
    let events = EventPublisherFactory::new(sqs_publisher_factory);

    Ok(ManagedServer {
        db_provider,
        db_cache,
        secrets,
        search,
        storage,
        events,
    })
}
