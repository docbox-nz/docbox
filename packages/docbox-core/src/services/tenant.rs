//! Business logic for interacting with a tenant

use crate::search::{SearchIndexFactory, TenantSearchIndex};
use crate::secrets::AppSecretManager;
use crate::storage::{StorageLayerFactory, TenantStorageLayer};
use anyhow::anyhow;
use docbox_database::models::tenant::TenantId;
use docbox_database::{
    connect_root_database, connect_tenant_database, models::tenant::Tenant,
    setup::create_tenant_tables, DatabasePoolCache, DbErr,
};
use std::ops::DerefMut;
use thiserror::Error;
use tracing::{debug, error};

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
    /// Failed to connect to the root db
    #[error("failed to connect")]
    ConnectRootDb(anyhow::Error),

    /// Failed to connect to the tenant db
    #[error("failed to connect")]
    ConnectTenantDb(anyhow::Error),

    /// Failed to start the database transaction
    #[error("failed to begin transaction")]
    BeginTransaction(DbErr),

    /// Error creating the tenant database row
    #[error("failed to create database tenant: {0}")]
    CreateTenant(anyhow::Error),

    /// Failed to create the tenant schema and tables
    #[error("failed to setup tenant database")]
    CreateTenantTables(DbErr),

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

    /// Failed to commit the database transaction
    #[error("failed to commit transaction")]
    CommitTransaction(DbErr),
}

#[derive(Default)]
pub struct InitTenantState {
    /// Storage layer if bucket created
    pub s3: Option<TenantStorageLayer>,

    /// Search index if search index is create
    pub search: Option<TenantSearchIndex>,
}

/// Attempts to initialize a new tenant
pub async fn initialize_tenant(
    db_cache: &DatabasePoolCache<AppSecretManager>,
    search: &SearchIndexFactory,
    storage: &StorageLayerFactory,
    create: CreateTenant,
    env: String,
    init_state: &mut InitTenantState,
) -> Result<Tenant, InitTenantError> {
    let root_db = connect_root_database(db_cache)
        .await
        .map_err(InitTenantError::ConnectRootDb)?;

    // Enter a database transaction
    let mut root_transaction = root_db
        .begin()
        .await
        .map_err(InitTenantError::BeginTransaction)?;

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
                return anyhow!("tenant already exists");
            }
        }

        anyhow::Error::new(err)
    })
    .map_err(InitTenantError::CreateTenant)?;

    let tenant_db = connect_tenant_database(db_cache, &tenant)
        .await
        .map_err(InitTenantError::ConnectTenantDb)?;

    // Enter a database transaction
    let mut tenant_transaction = tenant_db
        .begin()
        .await
        .map_err(InitTenantError::BeginTransaction)?;

    // Setup the tenant database
    create_tenant_tables(&mut tenant_transaction)
        .await
        .map_err(InitTenantError::CreateTenantTables)?;

    debug!("creating tenant s3 bucket");

    // Setup the tenant S3 bucket
    let storage = storage.create_storage_layer(&tenant);
    storage
        .create_bucket()
        .await
        .map_err(InitTenantError::CreateS3Bucket)?;
    init_state.s3 = Some(storage.clone());

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

    debug!("creating tenant search index");

    // Setup the tenant search index
    let search = search.create_search_index(&tenant);
    search
        .create_index()
        .await
        .map_err(InitTenantError::CreateSearchIndex)?;

    init_state.search = Some(search);

    // Commit database changes
    root_transaction
        .commit()
        .await
        .map_err(InitTenantError::CommitTransaction)?;

    // Commit database changes
    tenant_transaction
        .commit()
        .await
        .map_err(InitTenantError::CommitTransaction)?;

    Ok(tenant)
}

/// Attempts to rollback the creation of a tenant based on
/// the point of failure
pub async fn rollback_tenant_error(init_state: InitTenantState) {
    // Must revert created S3 bucket
    if let Some(s3) = init_state.s3 {
        if let Err(err) = s3.delete_bucket().await {
            error!("failed to rollback created tenant s3 bucket: {}", err);
        }
    }

    // Must revert created search index
    if let Some(search) = init_state.search {
        if let Err(err) = search.delete_index().await {
            error!("failed to rollback created tenant search index: {}", err);
        }
    }
}
