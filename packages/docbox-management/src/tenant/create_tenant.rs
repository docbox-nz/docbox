use crate::{
    database::{DatabaseProvider, close_pool_on_drop},
    password::random_password,
};
use docbox_core::{
    database::{
        DbErr, DbPool, DbResult, ROOT_DATABASE_NAME,
        create::{
            check_database_exists, check_database_role_exists, create_database,
            create_restricted_role, delete_database, delete_role,
        },
        migrations::apply_tenant_migrations,
        models::tenant::{Tenant, TenantId},
        utils::DatabaseErrorExt,
    },
    search::{SearchError, SearchIndexFactory, TenantSearchIndex},
    secrets::{SecretManager, SecretManagerError},
    storage::{CreateBucketOutcome, StorageLayer, StorageLayerError, StorageLayerFactory},
    tenant::tenant_options_ext::TenantOptionsExt,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::ops::DerefMut;
use thiserror::Error;

/// Errors that can occur when creating a tenant
#[derive(Debug, Error)]
pub enum CreateTenantError {
    /// Failed to connect to the temporary database
    #[error("error connecting to 'postgres' database: {0}")]
    ConnectPostgres(DbErr),

    /// Failed to create the tenant root database
    #[error("error creating tenant database: {0}")]
    CreateTenantDatabase(DbErr),

    /// Failed to connect to the created tenant database
    #[error("error connecting to tenant database: {0}")]
    ConnectTenantDatabase(DbErr),

    /// Failed to connect to the root database
    #[error("error connecting to root database: {0}")]
    ConnectRootDatabase(DbErr),

    /// Failed to create the tenant rol
    #[error("error creating tenant database role: {0}")]
    CreateTenantRole(DbErr),

    /// Database error
    #[error(transparent)]
    Database(#[from] DbErr),

    /// Failed to serialize the secret
    #[error("error serializing tenant secret: {0}")]
    SerializeSecret(serde_json::Error),

    /// The chosen database secret name is already in use
    #[error("failed to create tenant secret: secret name already exists")]
    SecretAlreadyExists,

    /// Failed to create the secret
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

/// Data required to rollback the failed creation of a tenant
#[derive(Default)]
struct CreateTenantRollbackData {
    search_index: Option<TenantSearchIndex>,
    storage: Option<StorageLayer>,
    secret: Option<(SecretManager, String)>,
    database: Option<String>,
    db_role: Option<String>,
}

impl CreateTenantRollbackData {
    async fn rollback(&mut self, db_provider: &impl DatabaseProvider) {
        // Rollback search index
        if let Some(search_index) = self.search_index.take()
            && let Err(error) = search_index.delete_index().await
        {
            tracing::error!(?error, "failed to rollback created tenant search index");
        }

        // Rollback storage
        if let Some(storage) = self.storage.take()
            && let Err(error) = storage.delete_bucket().await
        {
            tracing::error!(?error, "failed to rollback created tenant storage bucket");
        }

        // Rollback secrets
        if let Some((secrets, secret_name)) = self.secret.take()
            && let Err(error) = secrets.delete_secret(&secret_name, true).await
        {
            tracing::error!(?error, "failed to rollback tenant secret");
        }

        // Handle operations requiring a root database
        let db_name = self.database.take();
        let db_role_name = self.db_role.take();
        if db_name.is_some() || db_role_name.is_some() {
            match db_provider.connect("postgres").await {
                Ok(db_postgres) => {
                    // Rollback the database
                    if let Some(db_name) = db_name
                        && let Err(error) = delete_database(&db_postgres, &db_name).await
                    {
                        tracing::error!(?error, "failed to rollback tenant database");
                    }

                    // Rollback the database role
                    if let Some(db_role_name) = db_role_name
                        && let Err(error) = delete_role(&db_postgres, &db_role_name).await
                    {
                        tracing::error!(?error, "failed to rollback tenant db role name");
                    }

                    db_postgres.close().await;
                }
                Err(error) => {
                    tracing::error!(
                        ?error,
                        "failed to rollback tenant database, unable to acquire postgres database"
                    );
                }
            }
        }
    }
}

/// Handles the process of creating a new docbox tenant
///
/// Performs:
/// - Create tenant database
/// - Create tenant database role
/// - Store a secret with the tenant database role credentials
/// - Add the tenant to the docbox database
/// - Run migrations on the tenant database
/// - Create and setup the tenant storage bucket
/// - Create and setup the tenant search index
/// - Run search index migrations
///
/// On failure any created resources will be rolled back
#[tracing::instrument(skip_all, fields(?config))]
pub async fn create_tenant(
    db_provider: &impl DatabaseProvider,
    search_factory: &SearchIndexFactory,
    storage_factory: &StorageLayerFactory,
    secrets: &SecretManager,
    config: CreateTenantConfig,
) -> Result<Tenant, CreateTenantError> {
    let mut rollback = CreateTenantRollbackData::default();

    match create_tenant_inner(
        db_provider,
        search_factory,
        storage_factory,
        secrets,
        config,
        &mut rollback,
    )
    .await
    {
        Ok(value) => Ok(value),
        Err(error) => {
            // Rollback the failure
            rollback.rollback(db_provider).await;
            Err(error)
        }
    }
}

#[tracing::instrument(skip_all, fields(?config))]
async fn create_tenant_inner(
    db_provider: &impl DatabaseProvider,
    search_factory: &SearchIndexFactory,
    storage_factory: &StorageLayerFactory,
    secrets: &SecretManager,
    config: CreateTenantConfig,
    rollback: &mut CreateTenantRollbackData,
) -> Result<Tenant, CreateTenantError> {
    let (tenant_db, _tenant_db_guard) = {
        // Connect to the "postgres" database to use while creating the tenant database
        let db_postgres = db_provider
            .connect("postgres")
            .await
            .map_err(CreateTenantError::ConnectPostgres)?;
        let _postgres_guard = close_pool_on_drop(&db_postgres);

        // Create tenant database
        initialize_tenant_database(&db_postgres, &config.db_name, rollback).await?;
        tracing::info!("created tenant database");

        // Connect to the tenant database
        let tenant_db = db_provider
            .connect(&config.db_name)
            .await
            .map_err(CreateTenantError::ConnectTenantDatabase)?;

        let tenant_db_guard = close_pool_on_drop(&tenant_db);
        (tenant_db, tenant_db_guard)
    };

    // Generate password for the database role
    let db_role_password = random_password(30);

    initialize_tenant_db_role(
        &tenant_db,
        &config.db_name,
        &config.db_role_name,
        &db_role_password,
        rollback,
    )
    .await?;
    tracing::info!("created tenant user");

    initialize_tenant_db_secret(
        secrets,
        &config.db_secret_name,
        &config.db_role_name,
        &db_role_password,
        rollback,
    )
    .await?;
    tracing::info!("created tenant database secret");

    // Connect to the root database
    let root_db = db_provider
        .connect(ROOT_DATABASE_NAME)
        .await
        .map_err(CreateTenantError::ConnectRootDatabase)?;

    let _guard = close_pool_on_drop(&root_db);

    // Enter a database transaction
    let mut root_transaction = root_db
        .begin()
        .await
        .inspect_err(|error| tracing::error!(?error, "failed to begin root transaction"))?;

    // Create the tenant
    let tenant: Tenant = Tenant::create(
        root_transaction.deref_mut(),
        docbox_core::database::models::tenant::CreateTenant {
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
        // Handle attempts at a duplicate tenant creation
        if err.is_duplicate_record() {
            CreateTenantError::TenantAlreadyExist
        } else {
            CreateTenantError::Database(err)
        }
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
    create_tenant_storage(
        &tenant,
        storage_factory,
        config.storage_s3_queue_arn,
        config.storage_cors_origins,
        rollback,
    )
    .await?;

    // Setup the tenant search index
    tracing::debug!("creating tenant search index");
    let search = create_tenant_search(&tenant, search_factory, rollback).await?;

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
    let _guard = close_pool_on_drop(&db_postgres);

    check_database_exists(&db_postgres, db_name).await
}

/// Initializes the creation of a tenant database, if the database
/// already exists that silently passes. Returns a [DbPool] to the
/// tenant database
#[tracing::instrument(skip(db_postgres, rollback))]
async fn initialize_tenant_database(
    db_postgres: &DbPool,
    db_name: &str,
    rollback: &mut CreateTenantRollbackData,
) -> Result<(), CreateTenantError> {
    let already_exists = match create_database(db_postgres, db_name).await {
        // We created the database
        Ok(_) => false,
        // Database already exists
        Err(error) if error.is_database_exists() => true,
        // Other database error
        Err(error) => return Err(CreateTenantError::CreateTenantDatabase(error)),
    };

    if !already_exists {
        rollback.database = Some(db_name.to_string());
    }

    Ok(())
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

    let _guard = close_pool_on_drop(&db_postgres);

    check_database_role_exists(&db_postgres, role_name).await
}

/// Initializes a tenant db role that the docbox API will use when accessing
/// the tenant databases
#[tracing::instrument(skip(db, role_password, rollback))]
async fn initialize_tenant_db_role(
    db: &DbPool,
    db_name: &str,
    role_name: &str,
    role_password: &str,
    rollback: &mut CreateTenantRollbackData,
) -> Result<(), CreateTenantError> {
    // Setup the restricted root db role
    create_restricted_role(db, db_name, role_name, role_password)
        .await
        .map_err(CreateTenantError::CreateTenantRole)?;

    rollback.db_role = Some(role_name.to_string());

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
#[tracing::instrument(skip(secrets, role_password, rollback))]
async fn initialize_tenant_db_secret(
    secrets: &SecretManager,
    secret_name: &str,
    role_name: &str,
    role_password: &str,
    rollback: &mut CreateTenantRollbackData,
) -> Result<(), CreateTenantError> {
    // Ensure the secret does not already exist, we don't want to override it
    if secrets
        .has_secret(secret_name)
        .await
        .map_err(CreateTenantError::CreateTenantSecret)?
    {
        return Err(CreateTenantError::SecretAlreadyExists);
    }

    let secret_value = serde_json::to_string(&json!({
        "username": role_name,
        "password": role_password
    }))
    .map_err(CreateTenantError::SerializeSecret)?;

    secrets
        .set_secret(secret_name, &secret_value)
        .await
        .map_err(CreateTenantError::CreateTenantSecret)?;

    rollback.secret = Some((secrets.clone(), secret_name.to_string()));

    Ok(())
}

/// Create and setup the tenant storage
#[tracing::instrument(skip(storage, rollback))]
async fn create_tenant_storage(
    tenant: &Tenant,
    storage: &StorageLayerFactory,
    s3_queue_arn: Option<String>,
    origins: Vec<String>,
    rollback: &mut CreateTenantRollbackData,
) -> Result<(), CreateTenantError> {
    let storage = storage.create_layer(tenant.storage_layer_options());
    let outcome = storage
        .create_bucket()
        .await
        .inspect_err(|error| tracing::error!(?error, "failed to create tenant bucket"))
        .map_err(CreateTenantError::CreateStorageBucket)?;

    // Mark the storage for rollback if we created it
    if matches!(outcome, CreateBucketOutcome::New) {
        rollback.storage = Some(storage.clone());
    }

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

    Ok(())
}

/// Create and setup the tenant search index
#[tracing::instrument(skip(search, rollback))]
async fn create_tenant_search(
    tenant: &Tenant,
    search: &SearchIndexFactory,
    rollback: &mut CreateTenantRollbackData,
) -> Result<TenantSearchIndex, CreateTenantError> {
    // Setup the tenant search index
    let search = search.create_search_index(tenant);
    search
        .create_index()
        .await
        .map_err(CreateTenantError::CreateSearchIndex)
        .inspect_err(|error| tracing::error!(?error, "failed to create search index"))?;

    // Index has been created, provide it as rollback state
    rollback.search_index = Some(search.clone());

    Ok(search)
}
