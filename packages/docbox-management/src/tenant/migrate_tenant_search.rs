use crate::{
    database::{DatabaseProvider, close_pool_on_drop},
    tenant::get_pending_tenant_search_migrations::GetPendingTenantMigrationsError,
};
use docbox_database::{DbErr, ROOT_DATABASE_NAME, models::tenant::Tenant};
use docbox_search::{SearchError, SearchIndexFactory};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MigrateTenantSearchError {
    #[error("error connecting to root database: {0}")]
    ConnectRootDatabase(DbErr),

    #[error("error storing applied migration: {0}")]
    StoreMigration(DbErr),

    #[error(transparent)]
    GetPendingTenantMigrationsError(#[from] GetPendingTenantMigrationsError),

    #[error("failed to apply migration: {0}")]
    ApplyMigration(SearchError),

    #[error("error connecting to tenant database: {0}")]
    ConnectTenantDatabase(DbErr),

    #[error(transparent)]
    StartTransaction(DbErr),

    #[error(transparent)]
    CommitTransaction(DbErr),
}

#[tracing::instrument(skip(db_provider, search_factory))]
pub async fn migrate_tenant_search(
    db_provider: &impl DatabaseProvider,
    search_factory: &SearchIndexFactory,
    tenant: &Tenant,
    target_migration_name: Option<&str>,
) -> Result<(), MigrateTenantSearchError> {
    // Connect to the root database
    let root_db = db_provider
        .connect(ROOT_DATABASE_NAME)
        .await
        .map_err(MigrateTenantSearchError::ConnectRootDatabase)?;

    let _root_guard = close_pool_on_drop(&root_db);

    // Connect to the tenant database
    let tenant_db = db_provider
        .connect(&tenant.db_name)
        .await
        .map_err(MigrateTenantSearchError::ConnectTenantDatabase)?;

    let _tenant_guard = close_pool_on_drop(&tenant_db);

    let search = search_factory.create_search_index(tenant);

    // Start transactions
    let mut root_t = root_db
        .begin()
        .await
        .map_err(MigrateTenantSearchError::StartTransaction)?;
    let mut tenant_t = tenant_db
        .begin()
        .await
        .map_err(MigrateTenantSearchError::StartTransaction)?;

    search
        .apply_migrations(tenant, &mut root_t, &mut tenant_t, target_migration_name)
        .await
        .map_err(MigrateTenantSearchError::ApplyMigration)?;

    // Commit database transactions
    tenant_t
        .commit()
        .await
        .map_err(MigrateTenantSearchError::CommitTransaction)?;
    root_t
        .commit()
        .await
        .map_err(MigrateTenantSearchError::CommitTransaction)?;

    Ok(())
}
