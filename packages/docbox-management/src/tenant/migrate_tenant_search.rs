use docbox_database::models::tenant::Tenant;
use docbox_search::SearchIndexFactory;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MigrateTenantSearchError {
    #[error("failed to apply migration: {0}")]
    ApplyMigration(anyhow::Error),
}

pub async fn migrate_tenant_search(
    search_factory: &SearchIndexFactory,
    tenant: &Tenant,
    target_migration_name: &str,
) -> Result<(), MigrateTenantSearchError> {
    let search = search_factory.create_search_index(tenant);
    search
        .apply_migration(target_migration_name)
        .await
        .map_err(MigrateTenantSearchError::ApplyMigration)?;
    Ok(())
}
