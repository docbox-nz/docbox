use crate::{
    database::DatabaseProvider,
    tenant::{
        MigrateTenantsOutcome, TenantTarget,
        get_tenants::get_tenants,
        migrate_tenant_storage::{MigrateTenantStorageError, migrate_tenant_storage},
    },
};
use docbox_core::{
    database::{
        DbErr,
        models::tenant::{Tenant, TenantId},
    },
    storage::StorageLayerFactory,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MigrateTenantsStorageError {
    #[error("failed to get tenants: {0}")]
    GetTenants(DbErr),

    #[error(transparent)]
    MigrateTenant(#[from] MigrateTenantStorageError),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MigrateTenantsStorageConfig {
    /// Filter to a specific environment
    pub env: Option<String>,
    /// Filter to a specific tenant
    pub tenant_id: Option<TenantId>,
    /// Filter to skip failed migrations and continue
    pub skip_failed: bool,
    /// Specific migrations to run
    pub target_migration_name: Option<String>,
}

#[tracing::instrument(skip(db_provider, storage_factory))]
pub async fn migrate_tenants_storage(
    db_provider: &impl DatabaseProvider,
    storage_factory: &StorageLayerFactory,
    config: MigrateTenantsStorageConfig,
) -> Result<MigrateTenantsOutcome, MigrateTenantsStorageError> {
    let tenants = get_tenants(db_provider)
        .await
        .map_err(MigrateTenantsStorageError::GetTenants)?;

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
        let result = migrate_tenant_storage(
            db_provider,
            storage_factory,
            &tenant,
            config.target_migration_name.as_deref(),
        )
        .await;
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

                return Err(MigrateTenantsStorageError::MigrateTenant(error));
            }
        }
    }

    Ok(MigrateTenantsOutcome {
        applied_tenants,
        failed_tenants,
    })
}
