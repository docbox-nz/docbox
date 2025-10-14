use std::ops::DerefMut;

use docbox_core::utils::rollback::RollbackGuard;
use docbox_database::{
    DbErr, DbPool, DbResult, ROOT_DATABASE_NAME,
    create::{
        check_database_exists, check_database_role_exists, create_database, create_restricted_role,
    },
    migrations::apply_tenant_migrations,
    models::tenant::{Tenant, TenantId},
    utils::DatabaseErrorExt,
};
use docbox_search::{SearchError, SearchIndexFactory, TenantSearchIndex};
use docbox_secrets::{SecretManager, SecretManagerError};
use docbox_storage::{StorageLayerError, StorageLayerFactory};
use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

use crate::{database::DatabaseProvider, password::random_password};

/// Errors that can occur when creating a tenant
#[derive(Debug, Error)]
pub enum CreateTenantError {
    #[error("error connecting to 'postgres' database: {0}")]
    ConnectPostgres(DbErr),

    #[error("error creating tenant database: {0}")]
    CreateTenantDatabase(DbErr),

    #[error("error connecting to tenant database: {0}")]
    ConnectTenantDatabase(DbErr),

    #[error("error connecting to root database: {0}")]
    ConnectRootDatabase(DbErr),

    #[error("error creating tenant database role: {0}")]
    CreateTenantRole(DbErr),

    /// Database error
    #[error(transparent)]
    Database(#[from] DbErr),

    #[error("error serializing tenant secret: {0}")]
    SerializeSecret(serde_json::Error),

    #[error("failed to create tenant secret: {0}")]
    CreateTenantSecret(SecretManagerError),

    /// Tenant already exists
    #[error("tenant already exists")]
    TenantAlreadyExist,

    /// Failed to create the storage bucket
    #[error("failed to create tenant storage bucket: {0}")]
    CreateStorageBucket(StorageLayerError),

    /// Failed to setup the S3 bucket CORS rules
    #[error("failed to setup s3 notification rules: {0}")]
    SetupS3Notifications(StorageLayerError),

    /// Failed to setup the storage bucket CORS rules
    #[error("failed to setup storage origin rules rules: {0}")]
    SetupStorageOrigins(StorageLayerError),

    /// Failed to create the search index
    #[error("failed to create tenant search index: {0}")]
    CreateSearchIndex(SearchError),

    /// Failed to migrate the search index
    #[error("failed to migrate tenant search index: {0}")]
    MigrateSearchIndex(SearchError),
}

/// Request to create a tenant
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CreateTenantConfig {
    /// Unique ID for the tenant
    pub id: TenantId,
    /// Name of the tenant
    pub name: String,
    /// Environment of the tenant
    pub env: String,

    /// Database name for the tenant
    pub db_name: String,
    /// Database secret credentials name for the tenant
    /// (Where the username and password will be stored/)
    pub db_secret_name: String,
    /// Name for the tenant role
    pub db_role_name: String,

    /// Name of the tenant storage bucket
    pub storage_bucket_name: String,
    /// CORS Origins for setting up presigned uploads with S3
    pub storage_cors_origins: Vec<String>,
    /// ARN for the S3 queue to publish S3 notifications, required
    /// for presigned uploads
    pub storage_s3_queue_arn: Option<String>,

    /// Name of the tenant search index
    pub search_index_name: String,

    /// URL for the SQS event queue
    pub event_queue_url: Option<String>,
}

#[tracing::instrument(skip_all, fields(?config))]
pub async fn create_tenant(
    db_provider: &impl DatabaseProvider,
    search_factory: &SearchIndexFactory,
    storage_factory: &StorageLayerFactory,
    secrets: &SecretManager,
    config: CreateTenantConfig,
) -> Result<Tenant, CreateTenantError> {
    // Create tenant database
    let tenant_db = initialize_tenant_database(db_provider, &config.db_name).await?;
    tracing::info!("created tenant database");

    // Generate password for the database role
    let db_role_password = random_password(30);

    initialize_tenant_db_role(
        &tenant_db,
        &config.db_name,
        &config.db_role_name,
        &db_role_password,
    )
    .await?;
    tracing::info!("created tenant user");

    initialize_tenant_db_secret(
        secrets,
        &config.db_secret_name,
        &config.db_role_name,
        &db_role_password,
    )
    .await?;
    tracing::info!("created tenant database secret");

    // Connect to the root database
    let root_db = db_provider
        .connect(ROOT_DATABASE_NAME)
        .await
        .map_err(CreateTenantError::ConnectRootDatabase)?;

    // Enter a database transaction
    let mut root_transaction = root_db
        .begin()
        .await
        .inspect_err(|error| tracing::error!(?error, "failed to begin root transaction"))?;

    // Create the tenant
    let tenant: Tenant = Tenant::create(
        root_transaction.deref_mut(),
        docbox_database::models::tenant::CreateTenant {
            id: config.id,
            name: config.name,
            db_name: config.db_name,
            db_secret_name: config.db_secret_name,
            s3_name: config.storage_bucket_name,
            os_index_name: config.search_index_name,
            event_queue_url: config.event_queue_url,
            env: config.env,
        },
    )
    .await
    .map_err(|err| {
        if let Some(db_err) = err.as_database_error() {
            // Handle attempts at a duplicate tenant creation
            if db_err.is_unique_violation() {
                return CreateTenantError::TenantAlreadyExist;
            }
        }

        CreateTenantError::Database(err)
    })
    .inspect_err(|error| tracing::error!(?error, "failed to create tenant"))?;

    // Enter a database transaction
    let mut tenant_transaction = tenant_db
        .begin()
        .await
        .inspect_err(|error| tracing::error!(?error, "failed to begin tenant transaction"))?;

    // Setup the tenant database
    apply_tenant_migrations(
        &mut root_transaction,
        &mut tenant_transaction,
        &tenant,
        None,
    )
    .await
    .inspect_err(|error| tracing::error!(?error, "failed to create tenant tables"))?;

    // Setup the tenant storage bucket
    tracing::debug!("creating tenant storage");
    let storage = create_tenant_storage(
        &tenant,
        storage_factory,
        config.storage_s3_queue_arn,
        config.storage_cors_origins,
    )
    .await?;

    // Setup the tenant search index
    tracing::debug!("creating tenant search index");
    let (search, search_rollback) = create_tenant_search(&tenant, search_factory).await?;

    // Apply migrations to search
    search
        .apply_migrations(
            &tenant,
            &mut root_transaction,
            &mut tenant_transaction,
            None,
        )
        .await
        .map_err(CreateTenantError::MigrateSearchIndex)?;

    // Commit database changes
    tenant_transaction
        .commit()
        .await
        .inspect_err(|error| tracing::error!(?error, "failed to commit tenant transaction"))?;
    root_transaction
        .commit()
        .await
        .inspect_err(|error| tracing::error!(?error, "failed to commit root transaction"))?;

    // Commit search and storage
    storage.commit();
    search_rollback.commit();

    Ok(tenant)
}

/// Helper to check if a tenant database already exists
/// (Used to warn against duplicate creation when performing validation)
#[tracing::instrument(skip(db_provider))]
pub async fn is_tenant_database_existing(
    db_provider: &impl DatabaseProvider,
    db_name: &str,
) -> DbResult<bool> {
    // Connect to the "postgres" database to use while creating the tenant database
    let db_postgres = db_provider.connect("postgres").await?;
    check_database_exists(&db_postgres, db_name).await
}

/// Initializes the creation of a tenant database, if the database
/// already exists that silently passes. Returns a [DbPool] to the
/// tenant database
#[tracing::instrument(skip(db_provider))]
pub async fn initialize_tenant_database(
    db_provider: &impl DatabaseProvider,
    db_name: &str,
) -> Result<DbPool, CreateTenantError> {
    // Connect to the "postgres" database to use while creating the tenant database
    let db_postgres = db_provider
        .connect("postgres")
        .await
        .map_err(CreateTenantError::ConnectPostgres)?;

    // Create the tenant database
    if let Err(error) = create_database(&db_postgres, db_name).await
        && !error.is_database_exists()
    {
        return Err(CreateTenantError::CreateTenantDatabase(error));
    }

    // Connect to the tenant database
    let tenant_db = db_provider
        .connect(db_name)
        .await
        .map_err(CreateTenantError::ConnectTenantDatabase)?;

    Ok(tenant_db)
}

/// Helper to check if a tenant database role already exists
/// (Used to warn against duplicate creation when performing validation)
#[tracing::instrument(skip(db_provider))]
pub async fn is_tenant_database_role_existing(
    db_provider: &impl DatabaseProvider,
    role_name: &str,
) -> DbResult<bool> {
    // Connect to the "postgres" database to use while creating the tenant database
    let db_postgres = db_provider.connect("postgres").await?;
    check_database_role_exists(&db_postgres, role_name).await
}

/// Initializes a tenant db role that the docbox API will use when accessing
/// the tenant databases
#[tracing::instrument(skip(db, role_password))]
pub async fn initialize_tenant_db_role(
    db: &DbPool,
    db_name: &str,
    role_name: &str,
    role_password: &str,
) -> Result<(), CreateTenantError> {
    // Setup the restricted root db role
    create_restricted_role(db, db_name, role_name, role_password)
        .await
        .map_err(CreateTenantError::CreateTenantRole)?;

    Ok(())
}

/// Helper to check if a tenant database role secret already exists
/// (Used to warn against duplicate creation when performing validation)
#[tracing::instrument(skip(secrets))]
pub async fn is_tenant_database_role_secret_existing(
    secrets: &SecretManager,
    secret_name: &str,
) -> Result<bool, SecretManagerError> {
    secrets
        .get_secret(secret_name)
        .await
        .map(|value| value.is_some())
}

/// Initializes and stores the secret for the tenant database access
#[tracing::instrument(skip(secrets, role_password))]
pub async fn initialize_tenant_db_secret(
    secrets: &SecretManager,
    secret_name: &str,
    role_name: &str,
    role_password: &str,
) -> Result<(), CreateTenantError> {
    let secret_value = serde_json::to_string(&json!({
        "username": role_name,
        "password": role_password
    }))
    .map_err(CreateTenantError::SerializeSecret)?;

    secrets
        .set_secret(secret_name, &secret_value)
        .await
        .map_err(CreateTenantError::CreateTenantSecret)?;

    Ok(())
}

/// Create and setup the tenant storage
async fn create_tenant_storage(
    tenant: &Tenant,
    storage: &StorageLayerFactory,
    s3_queue_arn: Option<String>,
    origins: Vec<String>,
) -> Result<RollbackGuard<impl FnOnce()>, CreateTenantError> {
    let storage = storage.create_storage_layer(tenant);
    storage
        .create_bucket()
        .await
        .inspect_err(|error| tracing::error!(?error, "failed to create tenant bucket"))
        .map_err(CreateTenantError::CreateStorageBucket)?;

    let rollback = RollbackGuard::new({
        let storage = storage.clone();
        move || {
            tokio::spawn(async move {
                if let Err(error) = storage.delete_bucket().await {
                    tracing::error!(?error, "failed to rollback created tenant storage bucket");
                }
            });
        }
    });

    // Connect the S3 bucket for file upload notifications
    if let Some(s3_queue_arn) = s3_queue_arn {
        storage
            .add_bucket_notifications(&s3_queue_arn)
            .await
            .inspect_err(|error| {
                tracing::error!(?error, "failed to add bucket notification configuration")
            })
            .map_err(CreateTenantError::SetupS3Notifications)?;
    }

    // Setup bucket allowed origins for presigned uploads
    if !origins.is_empty() {
        storage
            .set_bucket_cors_origins(origins)
            .await
            .inspect_err(|error| tracing::error!(?error, "failed to add bucket cors rules"))
            .map_err(CreateTenantError::SetupStorageOrigins)?;
    }

    Ok(rollback)
}

/// Create and setup the tenant search index
async fn create_tenant_search(
    tenant: &Tenant,
    search: &SearchIndexFactory,
) -> Result<(TenantSearchIndex, RollbackGuard<impl FnOnce()>), CreateTenantError> {
    // Setup the tenant search index
    let search = search.create_search_index(tenant);
    search
        .create_index()
        .await
        .map_err(CreateTenantError::CreateSearchIndex)
        .inspect_err(|error| tracing::error!(?error, "failed to create search index"))?;

    let rollback = RollbackGuard::new({
        let search = search.clone();
        move || {
            // Search was not committed, roll back
            tokio::spawn(async move {
                if let Err(error) = search.delete_index().await {
                    tracing::error!(?error, "failed to rollback created tenant search index");
                }
            });
        }
    });

    Ok((search, rollback))
}
