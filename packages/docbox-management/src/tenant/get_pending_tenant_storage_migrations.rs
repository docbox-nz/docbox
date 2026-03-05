use crate::database::{DatabaseProvider, close_pool_on_drop};
use docbox_core::{
    database::{
        DbErr, ROOT_DATABASE_NAME,
        models::{tenant::Tenant, tenant_migration::TenantMigration},
    },
    storage::{StorageLayerError, StorageLayerFactory},
    tenant::tenant_options_ext::TenantOptionsExt,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GetPendingTenantMigrationsError {
    #[error(transparent)]
    Database(#[from] DbErr),

    #[error("failed to apply migration: {0}")]
    GetPendingMigrations(StorageLayerError),
}

#[tracing::instrument(skip(db_provider, storage_factory))]
pub async fn get_pending_tenant_storage_migrations(
    db_provider: &impl DatabaseProvider,
    storage_factory: &StorageLayerFactory,
    tenant: &Tenant,
) -> Result<Vec<String>, GetPendingTenantMigrationsError> {
    // Connect to the root database
    let root_db = db_provider.connect(ROOT_DATABASE_NAME).await?;
    let _guard = close_pool_on_drop(&root_db);

    let applied_migrations =
        TenantMigration::find_by_tenant(&root_db, tenant.id, &tenant.env).await?;
    let storage = storage_factory.create_layer(tenant.storage_layer_options());
    let migrations = storage
        .get_pending_migrations(
            applied_migrations
                .into_iter()
                .map(|value| value.name)
                .collect(),
        )
        .await
        .map_err(GetPendingTenantMigrationsError::GetPendingMigrations)?;

    Ok(migrations)
}
