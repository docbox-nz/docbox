use chrono::Utc;
use docbox_database::{
    DatabasePoolCache,
    models::{link_resolved_metadata::LinkResolvedMetadata, tenant::Tenant},
};
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PurgeExpiredWebsiteMetadataError {
    #[error("failed to connect to database")]
    ConnectDatabase,

    #[error("failed to query available tenants")]
    QueryTenants,
}

pub async fn safe_purge_expired_website_metadata(db_cache: Arc<DatabasePoolCache>) {
    if let Err(error) = purge_expired_website_metadata(db_cache).await {
        tracing::error!(
            ?error,
            "failed to purge expired website metadata for tenants"
        );
    }
}

#[tracing::instrument(skip_all)]
pub async fn purge_expired_website_metadata(
    db_cache: Arc<DatabasePoolCache>,
) -> Result<(), PurgeExpiredWebsiteMetadataError> {
    let tenants = {
        let db = db_cache.get_root_pool().await.map_err(|error| {
            tracing::error!(?error, "failed to connect to root database");
            PurgeExpiredWebsiteMetadataError::ConnectDatabase
        })?;

        Tenant::all(&db).await.map_err(|error| {
            tracing::error!(?error, "failed to query available tenants");
            PurgeExpiredWebsiteMetadataError::QueryTenants
        })?
    };

    for tenant in tenants {
        // Create the database connection pool
        let db = db_cache.get_tenant_pool(&tenant).await.map_err(|error| {
            tracing::error!(?error, "failed to connect to tenant database");
            PurgeExpiredWebsiteMetadataError::ConnectDatabase
        })?;

        let before = Utc::now();

        if let Err(error) = LinkResolvedMetadata::delete_expired(&db, before).await {
            tracing::error!(
                ?error,
                ?tenant,
                "failed to purge expired website metadata for tenant"
            );
        }
    }

    Ok(())
}
