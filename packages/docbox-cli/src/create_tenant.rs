use std::path::PathBuf;

use docbox_core::{
    aws::{aws_config, s3_client_from_env, SecretsManagerClient},
    search::{
        os::{create_open_search_prod, OpenSearchIndexFactory},
        SearchIndexFactory,
    },
    secrets::{aws::AwsSecretManager, memory::MemorySecretManager, AppSecretManager, Secret},
    storage::{s3::S3StorageLayerFactory, StorageLayerFactory},
    tenant::create_tenant::safe_create_tenant,
};
use docbox_database::{
    create::{create_database, create_tenant_user},
    models::tenant::TenantId,
    DatabasePoolCache,
};
use eyre::Context;
use serde::Deserialize;
use serde_json::json;
use url::Url;

use crate::{connect_db, Credentials};

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

    /// Skip secret creation (For local development)
    #[serde(default)]
    pub skip_secret_creation: bool,
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

pub async fn create_tenant(tenant_file: PathBuf) -> eyre::Result<()> {
    // Load CLI credentials
    let credentials_raw = tokio::fs::read("private/cli-credentials.json").await?;
    let credentials: Credentials = serde_json::from_slice(&credentials_raw)?;

    // Load the create tenant config
    let config_raw = tokio::fs::read(tenant_file).await?;
    let config: CreateTenant =
        serde_json::from_slice(&config_raw).context("failed to parse config")?;

    tracing::debug!(?config, "creating tenant");

    // Load AWS configuration
    let aws_config = aws_config().await;

    // Connect to the docbox database
    let db_docbox = connect_db(
        &credentials.host,
        credentials.port,
        &credentials.username,
        &credentials.password,
        "docbox",
    )
    .await
    .context("failed to connect to docbox database")?;

    // Create the tenant database
    if let Err(err) = create_database(&db_docbox, &config.db_name).await {
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
        &credentials.host,
        credentials.port,
        &credentials.username,
        &credentials.password,
        &config.db_name,
    )
    .await
    .context("failed to connect to tenant database")?;

    if !config.skip_role_creation {
        // Setup the tenant user
        create_tenant_user(
            &db_tenant,
            &config.db_name,
            &config.db_role_name,
            &config.db_password,
        )
        .await
        .context("failed to setup tenant user")?;
        tracing::info!("created tenant user");
    }

    // Create and store the new database secret
    let secret_value = json!({
        "username": config.db_role_name,
        "password": config.db_password
    });
    let secret_value = serde_json::to_string(&secret_value)?;

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

    secrets
        .create_secret(&config.db_secret_name, &secret_value)
        .await
        .map_err(|err| eyre::Error::msg(err.to_string()))?;

    tracing::info!("created database secret");

    // Setup database cache / connector
    let db_cache = DatabasePoolCache::new(
        credentials.host.clone(),
        credentials.port,
        // In the CLI the db credentials have high enough access to be used as the
        // "root secret"
        config.db_secret_name.clone(),
        secrets,
    );

    // Setup the search index
    let open_search_url = std::env::var("OPENSEARCH_URL")
        // Map the error to an anyhow type
        .context("missing OPENSEARCH_URL env")
        // Parse the URL
        .and_then(|url| Url::parse(&url).context("failed to parse OPENSEARCH_URL"))?;
    let open_search = create_open_search_prod(&aws_config, open_search_url)
        .map_err(|err| eyre::Error::msg(err.to_string()))?;
    let search_factory = SearchIndexFactory::new(OpenSearchIndexFactory::new(open_search));

    // Setup S3 access
    let s3_client =
        s3_client_from_env(&aws_config).map_err(|err| eyre::Error::msg(err.to_string()))?;
    let storage_factory = StorageLayerFactory::new(S3StorageLayerFactory::new(s3_client));

    // Attempt to initialize the tenant
    let tenant = safe_create_tenant(
        &db_cache,
        &search_factory,
        &storage_factory,
        docbox_core::tenant::create_tenant::CreateTenant {
            id: config.id,
            db_name: config.db_name,
            db_secret_name: config.db_secret_name,
            s3_name: config.s3_name,
            os_index_name: config.os_index_name,
            event_queue_url: config.event_queue_url,
            origins: config.origins,
            s3_queue_arn: config.s3_queue_arn,
        },
        config.env,
    )
    .await?;

    tracing::info!(?tenant, "tenant created successfully");

    Ok(())
}
