use docbox_core::{
    aws::aws_config, files::index_file::re_index_files, folders::index_folder::re_index_folders,
    links::index_link::re_index_links, search::SearchIndexFactory, secrets::AppSecretManager,
    storage::StorageLayerFactory,
};
use docbox_database::{models::tenant::Tenant, DatabasePoolCache};
use eyre::ContextCompat;
use uuid::Uuid;

use crate::CliConfiguration;

pub async fn rebuild_tenant_index(
    config: &CliConfiguration,
    env: String,
    tenant_id: Uuid,
) -> eyre::Result<()> {
    tracing::debug!(?env, ?tenant_id, "rebuilding tenant index");

    // Load AWS configuration
    let aws_config = aws_config().await;

    // Connect to secrets manager
    let secrets = AppSecretManager::from_config(&aws_config, config.secrets.clone());

    tracing::info!("created database secret");

    // Setup database cache / connector
    let db_cache = DatabasePoolCache::new(
        config.database.host.clone(),
        config.database.port,
        // In the CLI the db credentials have high enough access to be used as the
        // "root secret"
        "postgres/docbox/config".to_string(),
        secrets,
    );

    let search_factory = SearchIndexFactory::from_config(&aws_config, config.search.clone())
        .map_err(|err| eyre::Error::msg(err.to_string()))?;

    // Setup S3 access
    let storage_factory = StorageLayerFactory::from_config(&aws_config, config.storage.clone());

    let root_db = db_cache.get_root_pool().await?;
    let tenant = Tenant::find_by_id(&root_db, tenant_id, &env)
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
