use crate::storage::{StorageLayerFactory, TenantStorageLayer};
use docbox_database::migrations::apply_tenant_migrations;
use docbox_database::models::tenant::TenantId;
use docbox_database::{DbConnectErr, DbPool};
use docbox_database::{DbErr, models::tenant::Tenant};
use docbox_search::{SearchIndexFactory, TenantSearchIndex};
use std::ops::DerefMut;
use thiserror::Error;

/// Request to create a tenant
pub struct CreateTenant {
    /// Environment to create the tenant within
    pub env: String,

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

/// Attempts to initialize a new tenant
pub async fn create_tenant(
    root_db: &DbPool,
    tenant_db: &DbPool,

    search: &SearchIndexFactory,
    storage: &StorageLayerFactory,
    create: CreateTenant,
) -> Result<Tenant, InitTenantError> {
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
            env: create.env,
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
    let mut storage =
        create_tenant_storage(&tenant, storage, create.s3_queue_arn, create.origins).await?;

    // Setup the tenant search index
    tracing::debug!("creating tenant search index");
    let mut search = create_tenant_search(&tenant, search).await?;

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
    search.commit();

    Ok(tenant)
}

struct TenantStorageTransaction {
    storage: Option<TenantStorageLayer>,
}

impl TenantStorageTransaction {
    /// "Commit" the storage so that it won't be reverted on drop
    fn commit(&mut self) {
        _ = self.storage.take();
    }
}

impl Drop for TenantStorageTransaction {
    fn drop(&mut self) {
        if let Some(storage) = self.storage.take() {
            // Storage was not committed, roll back
            tokio::spawn(async move {
                if let Err(error) = storage.delete_bucket().await {
                    tracing::error!(?error, "failed to rollback created tenant storage bucket");
                }
            });
        }
    }
}

/// Create and setup the tenant storage
async fn create_tenant_storage(
    tenant: &Tenant,
    storage: &StorageLayerFactory,
    s3_queue_arn: Option<String>,
    origins: Vec<String>,
) -> Result<TenantStorageTransaction, InitTenantError> {
    let storage = storage.create_storage_layer(tenant);
    storage
        .create_bucket()
        .await
        .inspect_err(|error| tracing::error!(?error, "failed to create tenant bucket"))
        .map_err(InitTenantError::CreateS3Bucket)?;

    let transaction = TenantStorageTransaction {
        storage: Some(storage.clone()),
    };

    // Connect the S3 bucket for file upload notifications
    if let Some(s3_queue_arn) = s3_queue_arn {
        storage
            .add_bucket_notifications(&s3_queue_arn)
            .await
            .inspect_err(|error| {
                tracing::error!(?error, "failed to add bucket notification configuration")
            })
            .map_err(InitTenantError::SetupS3Notifications)?;
    }

    // Setup bucket allowed origins for presigned uploads
    if !origins.is_empty() {
        storage
            .add_bucket_cors(origins)
            .await
            .inspect_err(|error| tracing::error!(?error, "failed to add bucket cors rules"))
            .map_err(InitTenantError::SetupS3Cors)?;
    }

    Ok(transaction)
}

struct TenantSearchTransaction {
    index: Option<TenantSearchIndex>,
}

impl TenantSearchTransaction {
    /// "Commit" the index so that it won't be reverted on drop
    fn commit(&mut self) {
        _ = self.index.take();
    }
}

impl Drop for TenantSearchTransaction {
    fn drop(&mut self) {
        if let Some(index) = self.index.take() {
            // Storage was not committed, roll back
            tokio::spawn(async move {
                if let Err(error) = index.delete_index().await {
                    tracing::error!(?error, "failed to rollback created tenant search index");
                }
            });
        }
    }
}

/// Create and setup the tenant search
async fn create_tenant_search(
    tenant: &Tenant,
    search: &SearchIndexFactory,
) -> Result<TenantSearchTransaction, InitTenantError> {
    // Setup the tenant search index
    let search = search.create_search_index(tenant);
    search
        .create_index()
        .await
        .map_err(InitTenantError::CreateSearchIndex)
        .inspect_err(|error| tracing::error!(?error, "failed to create search index"))?;

    let transaction = TenantSearchTransaction {
        index: Some(search),
    };

    Ok(transaction)
}
