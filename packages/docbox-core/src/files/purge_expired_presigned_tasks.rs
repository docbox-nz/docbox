use crate::storage::{StorageLayerFactory, TenantStorageLayer};
use chrono::Utc;
use docbox_database::{
    DatabasePoolCache, DbPool,
    models::{
        presigned_upload_task::{PresignedTaskStatus, PresignedUploadTask},
        tenant::Tenant,
    },
};
use docbox_secrets::AppSecretManager;
use std::sync::Arc;

pub async fn safe_purge_expired_presigned_tasks(
    db_cache: Arc<DatabasePoolCache<AppSecretManager>>,
    storage: StorageLayerFactory,
) {
    if let Err(cause) = purge_expired_presigned_tasks(db_cache, storage).await {
        tracing::error!(?cause, "failed to purge presigned tasks");
    }
}

pub async fn purge_expired_presigned_tasks(
    db_cache: Arc<DatabasePoolCache<AppSecretManager>>,
    storage: StorageLayerFactory,
) -> anyhow::Result<()> {
    let db = db_cache.get_root_pool().await?;
    let tenants = Tenant::all(&db).await?;
    drop(db);

    for tenant in tenants {
        // Create the database connection pool
        let db = db_cache.get_tenant_pool(&tenant).await?;
        let storage = storage.create_storage_layer(&tenant);

        if let Err(cause) = purge_expired_presigned_tasks_tenant(&db, &storage).await {
            tracing::error!(
                ?cause,
                ?tenant,
                "failed to purge presigned tasks for tenant"
            );
        }
    }

    Ok(())
}

pub async fn purge_expired_presigned_tasks_tenant(
    db: &DbPool,
    storage: &TenantStorageLayer,
) -> anyhow::Result<()> {
    let current_date = Utc::now();
    let tasks = PresignedUploadTask::find_expired(db, current_date).await?;
    if tasks.is_empty() {
        return Ok(());
    }

    for task in tasks {
        // Delete the task itself
        if let Err(cause) = PresignedUploadTask::delete(db, task.id).await {
            tracing::error!(?cause, "failed to delete presigned upload task")
        }

        // Delete incomplete file uploads
        match task.status {
            PresignedTaskStatus::Completed { .. } => {
                // Upload completed, nothing to revert
            }
            PresignedTaskStatus::Failed { .. } | PresignedTaskStatus::Pending => {
                if let Err(cause) = storage.delete_file(&task.file_key).await {
                    tracing::error!(
                        ?cause,
                        "failed to delete expired presigned task file from tenant"
                    );
                }
            }
        }
    }

    Ok(())
}
