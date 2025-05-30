use std::path::PathBuf;

use docbox_core::{
    aws::aws_config, search::SearchIndexFactory, secrets::AppSecretManager,
    storage::StorageLayerFactory, tenant::create_tenant::safe_create_tenant,
};
use docbox_database::{
    create::{create_database, create_tenant_user},
    models::tenant::TenantId,
    DatabasePoolCache,
};
use eyre::Context;
use serde::Deserialize;
use serde_json::json;

use crate::{connect_db, CliConfiguration};

/// Request to create a tenant
#[derive(Debug, Deserialize)]
pub struct CreateTenant {
    /// Unique ID for the tenant
    pub id: TenantId,

    /// Database name for the tenant
    pub db_name: String,

    pub env: String,

    /// Database secret credentials name for the tenant
    /// (Where the username and password will be stored/)
    pub db_secret_name: String,

    pub db_role_name: String,
    pub db_password: String,

    #[serde(default)]
    pub skip_role_creation: bool,

    /// Name of the tenant s3 bucket
    pub s3_name: String,

    /// Name of the tenant search index
    pub os_index_name: String,

    /// URL for the SQS event queue
    pub event_queue_url: Option<String>,

    /// CORS Origins for setting up presigned uploads with S3
    pub origins: Vec<String>,

    /// ARN for the S3 queue to publish S3 notifications, required
    /// for presigned uploads
    pub s3_queue_arn: Option<String>,
}

pub async fn create_tenant(config: &CliConfiguration, tenant_file: PathBuf) -> eyre::Result<()> {
    // Load the create tenant config
    let tenant_config_raw = tokio::fs::read(tenant_file).await?;
    let tenant_config: CreateTenant =
        serde_json::from_slice(&tenant_config_raw).context("failed to parse config")?;

    tracing::debug!(?tenant_config, "creating tenant");

    // Load AWS configuration
    let aws_config = aws_config().await;

    // Connect to the docbox database
    let db_docbox = connect_db(
        &config.database.host,
        config.database.port,
        &config.database.username,
        &config.database.password,
        "docbox",
    )
    .await
    .context("failed to connect to docbox database")?;

    // Create the tenant database
    if let Err(err) = create_database(&db_docbox, &tenant_config.db_name).await {
        if !err
            .as_database_error()
            .is_some_and(|err| err.code().is_some_and(|code| code.to_string().eq("42P04")))
        {
            return Err(err.into());
        }
    }

    tracing::info!("created tenant database");

    // Connect to the tenant database
    let db_tenant = connect_db(
        &config.database.host,
        config.database.port,
        &config.database.username,
        &config.database.password,
        &tenant_config.db_name,
    )
    .await
    .context("failed to connect to tenant database")?;

    if !tenant_config.skip_role_creation {
        // Setup the tenant user
        create_tenant_user(
            &db_tenant,
            &tenant_config.db_name,
            &tenant_config.db_role_name,
            &tenant_config.db_password,
        )
        .await
        .context("failed to setup tenant user")?;
        tracing::info!("created tenant user");
    }

    // Create and store the new database secret
    let secret_value = json!({
        "username": tenant_config.db_role_name,
        "password": tenant_config.db_password
    });
    let secret_value = serde_json::to_string(&secret_value)?;

    let secrets = AppSecretManager::from_config(&aws_config, config.secrets.clone());
    secrets
        .create_secret(&tenant_config.db_secret_name, &secret_value)
        .await
        .map_err(|err| eyre::Error::msg(err.to_string()))?;

    tracing::info!("created database secret");

    // Setup database cache / connector
    let db_cache = DatabasePoolCache::new(
        config.database.host.clone(),
        config.database.port,
        // In the CLI the db credentials have high enough access to be used as the
        // "root secret"
        tenant_config.db_secret_name.clone(),
        secrets,
    );
    let search_factory = SearchIndexFactory::from_config(&aws_config, config.search.clone())
        .map_err(|err| eyre::Error::msg(err.to_string()))?;

    let storage_factory = StorageLayerFactory::from_config(&aws_config, config.storage.clone());

    // Attempt to initialize the tenant
    let tenant = safe_create_tenant(
        &db_cache,
        &search_factory,
        &storage_factory,
        docbox_core::tenant::create_tenant::CreateTenant {
            id: tenant_config.id,
            db_name: tenant_config.db_name,
            db_secret_name: tenant_config.db_secret_name,
            s3_name: tenant_config.s3_name,
            os_index_name: tenant_config.os_index_name,
            event_queue_url: tenant_config.event_queue_url,
            origins: tenant_config.origins,
            s3_queue_arn: tenant_config.s3_queue_arn,
        },
        tenant_config.env,
    )
    .await?;

    tracing::info!(?tenant, "tenant created successfully");

    Ok(())
}
