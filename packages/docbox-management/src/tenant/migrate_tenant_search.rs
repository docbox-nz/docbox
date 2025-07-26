use docbox_database::{
    DbErr, ROOT_DATABASE_NAME,
    models::{
        tenant::Tenant,
        tenant_migration::{CreateTenantMigration, TenantMigration},
    },
    sqlx::types::chrono::Utc,
};
use docbox_search::SearchIndexFactory;
use thiserror::Error;

use crate::{
    database::DatabaseProvider,
    tenant::get_pending_tenant_search_migrations::{
        GetPendingTenantMigrationsError, get_pending_tenant_search_migrations,
    },
};

#[derive(Debug, Error)]
pub enum MigrateTenantSearchError {
    #[error("error connecting to root database: {0}")]
    ConnectRootDatabase(DbErr),

    #[error("error storing applied migration: {0}")]
    StoreMigration(DbErr),

    #[error(transparent)]
    GetPendingTenantMigrationsError(#[from] GetPendingTenantMigrationsError),

    #[error("failed to apply migration: {0}")]
    ApplyMigration(anyhow::Error),
}

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

    let search = search_factory.create_search_index(tenant);
    let pending_migrations =
        get_pending_tenant_search_migrations(db_provider, search_factory, tenant).await?;

    for migration_name in pending_migrations {
        // If targeting a specific migration only apply the target one
        if target_migration_name
            .is_some_and(|target_migration_name| target_migration_name.ne(&migration_name))
        {
            continue;
        }

        search
            .apply_migration(&migration_name)
            .await
            .map_err(MigrateTenantSearchError::ApplyMigration)?;

        // Store the applied migration
        TenantMigration::create(
            &root_db,
            CreateTenantMigration {
                tenant_id: tenant.id,
                env: tenant.env.clone(),
                name: migration_name,
                applied_at: Utc::now(),
            },
        )
        .await
        .map_err(MigrateTenantSearchError::StoreMigration)?;
    }

    Ok(())
}
