use std::ops::DerefMut;

use crate::database::{DatabaseProvider, close_pool_on_drop};
use docbox_core::{
    database::{
        DbErr, DbTransaction, ROOT_DATABASE_NAME,
        models::{tenant::Tenant, tenant_migration::TenantMigration},
    },
    storage::{StorageLayer, StorageLayerError, StorageLayerFactory},
    tenant::tenant_options_ext::TenantOptionsExt,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MigrateTenantStorageError {
    #[error("error connecting to root database: {0}")]
    ConnectRootDatabase(DbErr),

    #[error("error storing applied migration: {0}")]
    StoreMigration(DbErr),

    #[error("failed to get pending migrations: {0}")]
    GetPendingMigrations(StorageLayerError),

    #[error("failed to apply migration: {0}")]
    ApplyMigration(StorageLayerError),

    #[error("error connecting to tenant database: {0}")]
    ConnectTenantDatabase(DbErr),

    #[error("error querying applied migrations: {0}")]
    GetAppliedMigrations(DbErr),

    #[error(transparent)]
    StartTransaction(DbErr),

    #[error(transparent)]
    CommitTransaction(DbErr),
}

#[tracing::instrument(skip(db_provider, storage_factory))]
pub async fn migrate_tenant_storage(
    db_provider: &impl DatabaseProvider,
    storage_factory: &StorageLayerFactory,
    tenant: &Tenant,
    target_migration_name: Option<&str>,
) -> Result<(), MigrateTenantStorageError> {
    // Connect to the root database
    let root_db = db_provider
        .connect(ROOT_DATABASE_NAME)
        .await
        .map_err(MigrateTenantStorageError::ConnectRootDatabase)?;

    let _root_guard = close_pool_on_drop(&root_db);

    // Connect to the tenant database
    let tenant_db = db_provider
        .connect(&tenant.db_name)
        .await
        .map_err(MigrateTenantStorageError::ConnectTenantDatabase)?;

    let _tenant_guard = close_pool_on_drop(&tenant_db);

    let storage = storage_factory.create_layer(tenant.storage_layer_options());

    // Start transaction
    let mut root_t = root_db
        .begin()
        .await
        .map_err(MigrateTenantStorageError::StartTransaction)?;

    migrate_tenant_storage_inner(&storage, &mut root_t, tenant, target_migration_name).await?;

    // Commit database transaction
    root_t
        .commit()
        .await
        .map_err(MigrateTenantStorageError::CommitTransaction)?;

    Ok(())
}

#[tracing::instrument(skip(storage, root_t))]
pub async fn migrate_tenant_storage_inner(
    storage: &StorageLayer,
    root_t: &mut DbTransaction<'_>,
    tenant: &Tenant,
    target_migration_name: Option<&str>,
) -> Result<(), MigrateTenantStorageError> {
    let applied_migrations =
        TenantMigration::find_by_tenant(root_t.deref_mut(), tenant.id, &tenant.env)
            .await
            .inspect_err(|error| tracing::error!(?error, "failed to query tenant migrations"))
            .map_err(MigrateTenantStorageError::GetAppliedMigrations)?;

    let pending_migrations = storage
        .get_pending_migrations(
            applied_migrations
                .into_iter()
                .map(|value| value.name)
                .collect(),
        )
        .await
        .map_err(MigrateTenantStorageError::GetPendingMigrations)?;

    for migration_name in pending_migrations {
        // If targeting a specific migration only apply the target one
        if target_migration_name
            .is_some_and(|target_migration_name| target_migration_name.ne(&migration_name))
        {
            continue;
        }

        // Apply the migration
        if let Err(error) = storage.apply_migration(&migration_name).await {
            tracing::error!(%migration_name, ?error, "failed to apply migration");
            return Err(MigrateTenantStorageError::ApplyMigration(error));
        }
    }

    Ok(())
}
