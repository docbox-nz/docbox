use chrono::Utc;
use docbox_database::{
    DatabasePoolCache, DbPool, DbResult,
    models::{
        presigned_upload_task::{PresignedTaskStatus, PresignedUploadTask},
        tenant::Tenant,
    },
};
use docbox_storage::{StorageLayer, StorageLayerFactory};
use std::sync::Arc;
use thiserror::Error;

use crate::tenant::tenant_options_ext::TenantOptionsExt;

pub async fn safe_purge_expired_presigned_tasks(
    db_cache: Arc<DatabasePoolCache>,
    storage: StorageLayerFactory,
) {
    if let Err(error) = purge_expired_presigned_tasks(db_cache, storage).await {
        tracing::error!(?error, "failed to purge presigned tasks");
    }
}

#[derive(Debug, Error)]
pub enum PurgeExpiredPresignedError {
    #[error("failed to connect to database")]
    ConnectDatabase,

    #[error("failed to query available tenants")]
    QueryTenants,
}

#[tracing::instrument(skip_all)]
pub async fn purge_expired_presigned_tasks(
    db_cache: Arc<DatabasePoolCache>,
    storage: StorageLayerFactory,
) -> Result<(), PurgeExpiredPresignedError> {
    let db = db_cache.get_root_pool().await.map_err(|error| {
        tracing::error!(?error, "failed to connect to root database");
        PurgeExpiredPresignedError::ConnectDatabase
    })?;

    let tenants = Tenant::all(&db).await.map_err(|error| {
        tracing::error!(?error, "failed to query available tenants");
        PurgeExpiredPresignedError::QueryTenants
    })?;

    // Early drop the root database pool access
    drop(db);

    for tenant in tenants {
        // Create the database connection pool
        let db = db_cache.get_tenant_pool(&tenant).await.map_err(|error| {
            tracing::error!(?error, "failed to connect to tenant database");
            PurgeExpiredPresignedError::ConnectDatabase
        })?;

        let storage = storage.create_layer(tenant.storage_layer_options());

        if let Err(error) = purge_expired_presigned_tasks_tenant(&db, &storage).await {
            tracing::error!(
                ?error,
                ?tenant,
                "failed to purge presigned tasks for tenant"
            );
        }
    }

    Ok(())
}

pub async fn purge_expired_presigned_tasks_tenant(
    db: &DbPool,
    storage: &StorageLayer,
) -> DbResult<()> {
    let current_date = Utc::now();
    let tasks = PresignedUploadTask::find_expired(db, current_date).await?;
    if tasks.is_empty() {
        return Ok(());
    }

    for task in tasks {
        // Delete the task itself
        if let Err(error) = PresignedUploadTask::delete(db, task.id).await {
            tracing::error!(?error, "failed to delete presigned upload task");
        }

        // Delete incomplete file uploads
        match task.status {
            PresignedTaskStatus::Completed { .. } => {
                // Upload completed, nothing to revert
            }
            PresignedTaskStatus::Failed { .. } | PresignedTaskStatus::Pending => {
                if let Err(error) = storage.delete_file(&task.file_key).await {
                    tracing::error!(
                        ?error,
                        "failed to delete expired presigned task file from tenant"
                    );
                }
            }
        }
    }

    Ok(())
}
