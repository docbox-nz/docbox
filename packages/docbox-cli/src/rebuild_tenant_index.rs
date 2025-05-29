use std::path::PathBuf;

use docbox_core::{
    aws::{aws_config, s3_client_from_env, SecretsManagerClient},
    files::index_file::re_index_files,
    folders::index_folder::re_index_folders,
    links::index_link::re_index_links,
    search::SearchIndexFactory,
    secrets::{aws::AwsSecretManager, memory::MemorySecretManager, AppSecretManager, Secret},
    storage::{s3::S3StorageLayerFactory, StorageLayerFactory},
};
use docbox_database::{models::tenant::Tenant, DatabasePoolCache};
use eyre::{Context, ContextCompat};
use serde_json::json;

use crate::{create_tenant::CreateTenant, Credentials};

pub async fn rebuild_tenant_index(tenant_file: PathBuf) -> eyre::Result<()> {
    // Load CLI credentials
    let credentials_raw = tokio::fs::read("private/cli-credentials.prod.json").await?;
    let credentials: Credentials = serde_json::from_slice(&credentials_raw)?;

    // Load the create tenant config
    let config_raw = tokio::fs::read(tenant_file).await?;
    let config: CreateTenant =
        serde_json::from_slice(&config_raw).context("failed to parse config")?;

    tracing::debug!(?config, "creating tenant");

    // Load AWS configuration
    let aws_config = aws_config().await;

    // Connect to secrets manager
    let secrets_client = SecretsManagerClient::new(&aws_config);
    let secrets = match config.skip_secret_creation {
        false => AppSecretManager::Aws(AwsSecretManager::new(secrets_client)),
        true => AppSecretManager::Memory(MemorySecretManager::new(
            [(
                config.db_secret_name.to_string(),
                Secret::String(serde_json::to_string(&json!({
                    "username": config.db_role_name,
                    "password": config.db_password
                }))?),
            )]
            .into_iter()
            .collect(),
            None,
        )),
    };

    tracing::info!("created database secret");

    // Setup database cache / connector
    let db_cache = DatabasePoolCache::new(
        credentials.host.clone(),
        credentials.port,
        // In the CLI the db credentials have high enough access to be used as the
        // "root secret"
        "postgres/docbox/config".to_string(),
        secrets,
    );

    let search_factory = SearchIndexFactory::from_env(&aws_config)
        .map_err(|err| eyre::Error::msg(err.to_string()))?;

    // Setup S3 access
    let s3_client =
        s3_client_from_env(&aws_config).map_err(|err| eyre::Error::msg(err.to_string()))?;
    let storage_factory = StorageLayerFactory::new(S3StorageLayerFactory::new(s3_client));

    let root_db = db_cache.get_root_pool().await?;
    let tenant = Tenant::find_by_id(&root_db, config.id, &config.env)
        .await?
        .context("tenant not found")?;

    let db = db_cache.get_tenant_pool(&tenant).await?;
    let search = search_factory.create_search_index(&tenant);
    let storage = storage_factory.create_storage_layer(&tenant);

    tracing::info!(?tenant, "started re-indexing tenant");

    re_index_links(&db, &search, &tenant)
        .await
        .inspect_err(|error| tracing::error!(?error, "failed to re-index links"))
        .map_err(|err| eyre::Error::msg(err.to_string()))?;

    re_index_folders(&db, &search, &tenant)
        .await
        .inspect_err(|error| tracing::error!(?error, "failed to re-index folders"))
        .map_err(|err| eyre::Error::msg(err.to_string()))?;

    re_index_files(&db, &search, &storage, &tenant)
        .await
        .inspect_err(|error| tracing::error!(?error, "failed to re-index files"))
        .map_err(|err| eyre::Error::msg(err.to_string()))?;

    Ok(())
}
