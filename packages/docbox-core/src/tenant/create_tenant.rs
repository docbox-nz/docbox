//! Business logic for interacting with a tenant

use crate::search::{SearchIndexFactory, TenantSearchIndex};
use crate::secrets::AppSecretManager;
use crate::storage::{StorageLayerFactory, TenantStorageLayer};
use docbox_database::models::tenant::TenantId;
use docbox_database::DbConnectErr;
use docbox_database::{
    models::tenant::Tenant, setup::create_tenant_tables, DatabasePoolCache, DbErr,
};
use std::ops::DerefMut;
use thiserror::Error;

/// Request to create a tenant
pub struct CreateTenant {
    /// Unique ID for the tenant
    pub id: TenantId,

    /// Database name for the tenant
    pub db_name: String,

    /// Database secret credentials name for the tenant
    pub db_secret_name: String,

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

#[derive(Debug, Error)]
pub enum InitTenantError {
    /// Failed to connect to a database
    #[error("failed to connect")]
    ConnectDb(#[from] DbConnectErr),

    /// Database error
    #[error(transparent)]
    Database(#[from] DbErr),

    /// Tenant already exists
    #[error("tenant already exists")]
    TenantAlreadyExist,

    /// Failed to create the S3 bucket
    #[error("failed to create tenant s3 bucket: {0}")]
    CreateS3Bucket(anyhow::Error),

    /// Failed to setup the S3 bucket CORS rules
    #[error("failed to setup s3 notification rules: {0}")]
    SetupS3Notifications(anyhow::Error),

    /// Failed to setup the S3 bucket CORS rules
    #[error("failed to setup s3 CORS rules: {0}")]
    SetupS3Cors(anyhow::Error),

    /// Failed to create the search index
    #[error("failed to create tenant search index: {0}")]
    CreateSearchIndex(anyhow::Error),
}

#[derive(Default)]
struct CreateTenantState {
    /// Storage layer if bucket created
    storage: Option<TenantStorageLayer>,

    /// Search index if search index is create
    search: Option<TenantSearchIndex>,
}

/// Attempts to initialize a new tenant
pub async fn safe_create_tenant(
    db_cache: &DatabasePoolCache<AppSecretManager>,
    search: &SearchIndexFactory,
    storage: &StorageLayerFactory,
    create: CreateTenant,
    env: String,
) -> Result<Tenant, InitTenantError> {
    let mut create_state = CreateTenantState::default();

    create_tenant(db_cache, search, storage, create, env, &mut create_state)
        .await
        .inspect_err(|_| {
            // Attempt to rollback any allocated resources in the background
            tokio::spawn(rollback_tenant_error(create_state));
        })
}

/// Attempts to initialize a new tenant
async fn create_tenant(
    db_cache: &DatabasePoolCache<AppSecretManager>,
    search: &SearchIndexFactory,
    storage: &StorageLayerFactory,
    create: CreateTenant,
    env: String,
    create_state: &mut CreateTenantState,
) -> Result<Tenant, InitTenantError> {
    let root_db = db_cache
        .get_root_pool()
        .await
        .inspect_err(|error| tracing::error!(?error, "failed to connect to root database"))?;

    // Enter a database transaction
    let mut root_transaction = root_db
        .begin()
        .await
        .inspect_err(|error| tracing::error!(?error, "failed to begin root transaction"))?;

    // Create the tenant
    let tenant: Tenant = Tenant::create(
        root_transaction.deref_mut(),
        docbox_database::models::tenant::CreateTenant {
            id: create.id,
            db_name: create.db_name,
            db_secret_name: create.db_secret_name,
            s3_name: create.s3_name,
            os_index_name: create.os_index_name,
            event_queue_url: create.event_queue_url,
            env,
        },
    )
    .await
    .map_err(|err| {
        if let Some(db_err) = err.as_database_error() {
            // Handle attempts at a duplicate tenant creation
            if db_err.is_unique_violation() {
                return InitTenantError::TenantAlreadyExist;
            }
        }

        InitTenantError::Database(err)
    })
    .inspect_err(|error| tracing::error!(?error, "failed to create tenant"))?;

    let tenant_db = db_cache
        .get_tenant_pool(&tenant)
        .await
        .inspect_err(|error| tracing::error!(?error, "failed to connect to tenant database"))?;

    // Enter a database transaction
    let mut tenant_transaction = tenant_db
        .begin()
        .await
        .inspect_err(|error| tracing::error!(?error, "failed to begin tenant transaction"))?;

    // Setup the tenant database
    create_tenant_tables(&mut tenant_transaction)
        .await
        .inspect_err(|error| tracing::error!(?error, "failed to create tenant tables"))?;

    tracing::debug!("creating tenant s3 bucket");

    // Setup the tenant S3 bucket
    let storage = storage.create_storage_layer(&tenant);
    storage
        .create_bucket()
        .await
        .map_err(InitTenantError::CreateS3Bucket)?;
    create_state.storage = Some(storage.clone());

    // Connect the S3 bucket for file upload notifications
    if let Some(s3_queue_arn) = create.s3_queue_arn {
        if let Err(cause) = storage.add_bucket_notifications(&s3_queue_arn).await {
            tracing::error!(?cause, "failed to add bucket notification configuration");
            return Err(InitTenantError::SetupS3Notifications(cause));
        };
    }

    // Setup bucket allowed origins for presigned uploads
    if !create.origins.is_empty() {
        if let Err(cause) = storage.add_bucket_cors(create.origins).await {
            tracing::error!(?cause, "failed to add bucket cors rules");
            return Err(InitTenantError::SetupS3Cors(cause));
        }
    }

    tracing::debug!("creating tenant search index");

    // Setup the tenant search index
    let search = search.create_search_index(&tenant);
    search
        .create_index()
        .await
        .map_err(InitTenantError::CreateSearchIndex)
        .inspect_err(|error| tracing::error!(?error, "failed to create search index"))?;

    create_state.search = Some(search);

    // Commit database changes
    root_transaction
        .commit()
        .await
        .inspect_err(|error| tracing::error!(?error, "failed to commit root transaction"))?;

    // Commit database changes
    tenant_transaction
        .commit()
        .await
        .inspect_err(|error| tracing::error!(?error, "failed to commit tenant transaction"))?;

    Ok(tenant)
}

/// Attempts to rollback the creation of a tenant based on
/// the point of failure
async fn rollback_tenant_error(create_state: CreateTenantState) {
    // Must revert created S3 bucket
    if let Some(storage) = create_state.storage {
        if let Err(error) = storage.delete_bucket().await {
            tracing::error!(?error, "failed to rollback created tenant s3 bucket");
        }
    }

    // Must revert created search index
    if let Some(search) = create_state.search {
        if let Err(error) = search.delete_index().await {
            tracing::error!(?error, "failed to rollback created tenant search index");
        }
    }
}
