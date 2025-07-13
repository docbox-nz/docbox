use docbox_database::{
    DbErr,
    models::tenant::{Tenant, TenantId},
};
use docbox_search::SearchIndexFactory;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    database::DatabaseProvider,
    tenant::{
        MigrateTenantsOutcome, TenantTarget,
        get_tenants::get_tenants,
        migrate_tenant_search::{MigrateTenantSearchError, migrate_tenant_search},
    },
};

#[derive(Debug, Error)]
pub enum MigrateTenantsSearchError {
    #[error("failed to get tenants: {0}")]
    GetTenants(DbErr),

    #[error(transparent)]
    MigrateTenant(MigrateTenantSearchError),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MigrateTenantsSearchConfig {
    /// Filter to a specific environment
    pub env: Option<String>,
    /// Filter to a specific tenant
    pub tenant_id: Option<TenantId>,
    /// Filter to skip failed migrations and continue
    pub skip_failed: bool,
    /// Specific migrations to run
    pub target_migration_name: String,
}

pub async fn migrate_tenants_search(
    db_provider: &impl DatabaseProvider,
    search_factory: &SearchIndexFactory,
    config: MigrateTenantsSearchConfig,
) -> Result<MigrateTenantsOutcome, MigrateTenantsSearchError> {
    let tenants = get_tenants(db_provider)
        .await
        .map_err(MigrateTenantsSearchError::GetTenants)?;

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
        let target_migration_name = config.target_migration_name.as_str();
        let result = migrate_tenant_search(search_factory, &tenant, target_migration_name).await;
        match result {
            Ok(_) => {
                applied_tenants.push(TenantTarget {
                    env: tenant.env,
                    tenant_id: tenant.id,
                });
            }
            Err(error) => {
                failed_tenants.push(TenantTarget {
                    env: tenant.env,
                    tenant_id: tenant.id,
                });

                tracing::error!(?error, "failed to apply tenant migration");

                if !config.skip_failed {
                    tracing::debug!(?applied_tenants, "completed migrations");
                    break;
                }

                return Err(MigrateTenantsSearchError::MigrateTenant(error));
            }
        }
    }

    Ok(MigrateTenantsOutcome {
        applied_tenants,
        failed_tenants,
    })
}
