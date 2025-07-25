use crate::database::DatabaseProvider;
use docbox_database::{
    DbErr, ROOT_DATABASE_NAME,
    models::{tenant::Tenant, tenant_migration::TenantMigration},
};
use docbox_search::SearchIndexFactory;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GetPendingTenantMigrationsError {
    #[error(transparent)]
    Database(#[from] DbErr),

    #[error("failed to apply migration: {0}")]
    GetPendingMigrations(anyhow::Error),
}

pub async fn get_pending_tenant_search_migrations(
    db_provider: &impl DatabaseProvider,
    search_factory: &SearchIndexFactory,
    tenant: &Tenant,
) -> Result<Vec<String>, GetPendingTenantMigrationsError> {
    // Connect to the root database
    let root_db = db_provider.connect(ROOT_DATABASE_NAME).await?;
    let applied_migrations =
        TenantMigration::find_by_tenant(&root_db, tenant.id, &tenant.env).await?;
    let search = search_factory.create_search_index(tenant);
    let migrations = search
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
