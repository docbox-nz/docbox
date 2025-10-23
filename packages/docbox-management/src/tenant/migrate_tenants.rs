use crate::{
    database::{DatabaseProvider, close_pool_on_drop},
    tenant::{
        MigrateTenantsOutcome, TenantTarget,
        migrate_tenant::{MigrateTenantError, migrate_tenant},
    },
};
use docbox_database::{
    DbErr, ROOT_DATABASE_NAME,
    models::tenant::{Tenant, TenantId},
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MigrateTenantsError {
    #[error("error connecting to root database: {0}")]
    ConnectRootDatabase(DbErr),

    #[error("failed to get tenants: {0}")]
    GetTenants(DbErr),

    #[error(transparent)]
    MigrateTenant(MigrateTenantError),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MigrateTenantsConfig {
    /// Filter to a specific environment
    pub env: Option<String>,
    /// Filter to a specific tenant
    pub tenant_id: Option<TenantId>,
    /// Filter to skip failed migrations and continue
    pub skip_failed: bool,
    /// Specific migrations to run
    pub target_migration_name: Option<String>,
}

#[tracing::instrument(skip(db_provider))]
pub async fn migrate_tenants(
    db_provider: &impl DatabaseProvider,
    config: MigrateTenantsConfig,
) -> Result<MigrateTenantsOutcome, MigrateTenantsError> {
    // Connect to the root database
    let root_db = db_provider
        .connect(ROOT_DATABASE_NAME)
        .await
        .map_err(MigrateTenantsError::ConnectRootDatabase)?;

    let _guard = close_pool_on_drop(&root_db);

    let tenants = Tenant::all(&root_db)
        .await
        .map_err(MigrateTenantsError::GetTenants)?;

    // Filter to our desired tenants
    let tenants: Vec<Tenant> = tenants
        .into_iter()
        .filter(|tenant| {
            if config.env.as_ref().is_some_and(|env| tenant.env.ne(env)) {
                return false;
            }

            if config
                .tenant_id
                .as_ref()
                .is_some_and(|schema| tenant.id.ne(schema))
            {
                return false;
            }

            true
        })
        .collect();

    let mut applied_tenants = Vec::new();
    let mut failed_tenants = Vec::new();

    for tenant in tenants {
        let target_migration_name = config.target_migration_name.as_deref();
        let result = migrate_tenant(db_provider, &tenant, target_migration_name).await;
        match result {
            Ok(_) => {
                applied_tenants.push(TenantTarget {
                    env: tenant.env,
                    name: tenant.name,
                    tenant_id: tenant.id,
                });
            }
            Err(error) => {
                failed_tenants.push((
                    error.to_string(),
                    TenantTarget {
                        env: tenant.env,
                        name: tenant.name,
                        tenant_id: tenant.id,
                    },
                ));

                tracing::error!(?error, "failed to apply tenant migration");

                if !config.skip_failed {
                    tracing::debug!(?applied_tenants, "completed migrations");
                    break;
                }

                return Err(MigrateTenantsError::MigrateTenant(error));
            }
        }
    }

    Ok(MigrateTenantsOutcome {
        applied_tenants,
        failed_tenants,
    })
}
