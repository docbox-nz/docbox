use chrono::{Days, Utc};
use docbox_database::{
    DatabasePoolCache,
    models::{tasks::Task, tenant::Tenant},
};
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PurgeExpiredTaskError {
    #[error("failed to connect to database")]
    ConnectDatabase,

    #[error("failed to query available tenants")]
    QueryTenants,
}

pub async fn safe_purge_expired_tasks(db_cache: Arc<DatabasePoolCache>) {
    if let Err(error) = purge_expired_tasks(db_cache).await {
        tracing::error!(?error, "failed to purge expired tasks for tenants");
    }
}

#[tracing::instrument(skip_all)]
pub async fn purge_expired_tasks(
    db_cache: Arc<DatabasePoolCache>,
) -> Result<(), PurgeExpiredTaskError> {
    let tenants = {
        let db = db_cache.get_root_pool().await.map_err(|error| {
            tracing::error!(?error, "failed to connect to root database");
            PurgeExpiredTaskError::ConnectDatabase
        })?;

        Tenant::all(&db).await.map_err(|error| {
            tracing::error!(?error, "failed to query available tenants");
            PurgeExpiredTaskError::QueryTenants
        })?
    };

    for tenant in tenants {
        // Create the database connection pool
        let db = db_cache.get_tenant_pool(&tenant).await.map_err(|error| {
            tracing::error!(?error, "failed to connect to tenant database");
            PurgeExpiredTaskError::ConnectDatabase
        })?;

        let before = match Utc::now().checked_sub_days(Days::new(30)) {
            Some(value) => value,
            None => {
                tracing::error!("time underflow while attempting to compute task expiry date");
                continue;
            }
        };

        if let Err(error) = Task::delete_expired(&db, before).await {
            tracing::error!(?error, ?tenant, "failed to purge expired tasks for tenant");
        }
    }

    Ok(())
}
