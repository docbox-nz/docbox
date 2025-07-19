use crate::database::DatabaseProvider;
use docbox_database::{
    DbErr, ROOT_DATABASE_NAME, migrations::apply_tenant_migrations, models::tenant::Tenant,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MigrateTenantError {
    #[error("error connecting to root database: {0}")]
    ConnectRootDatabase(DbErr),

    #[error("error connecting to tenant database: {0}")]
    ConnectTenantDatabase(DbErr),

    #[error("failed to apply migrations: {0}")]
    ApplyMigration(DbErr),

    #[error(transparent)]
    StartTransaction(DbErr),

    #[error(transparent)]
    CommitTransaction(DbErr),
}

pub async fn migrate_tenant(
    db_provider: &impl DatabaseProvider,
    tenant: &Tenant,
    target_migration_name: Option<&str>,
) -> Result<(), MigrateTenantError> {
    // Connect to the root database
    let root_db = db_provider
        .connect(ROOT_DATABASE_NAME)
        .await
        .map_err(MigrateTenantError::ConnectRootDatabase)?;

    // Connect to the tenant database
    let tenant_db = db_provider
        .connect(&tenant.db_name)
        .await
        .map_err(MigrateTenantError::ConnectTenantDatabase)?;

    // Start transactions
    let mut root_t = root_db
        .begin()
        .await
        .map_err(MigrateTenantError::StartTransaction)?;
    let mut tenant_t = tenant_db
        .begin()
        .await
        .map_err(MigrateTenantError::StartTransaction)?;

    // Apply migrations
    apply_tenant_migrations(&mut root_t, &mut tenant_t, tenant, target_migration_name)
        .await
        .map_err(MigrateTenantError::ApplyMigration)?;

    // Commit database transactions
    tenant_t
        .commit()
        .await
        .map_err(MigrateTenantError::CommitTransaction)?;
    root_t
        .commit()
        .await
        .map_err(MigrateTenantError::CommitTransaction)?;

    Ok(())
}
