use docbox_core::{
    database::{DbErr, models::tenant::Tenant, sqlx::types::Uuid},
    storage::StorageLayerError,
    tenant::tenant_options_ext::TenantOptionsExt,
};
use docbox_management_interface::{
    CheckRootOutput, CreateTenantInput, CreateTenantOutput, DeleteTenantInput, DeleteTenantOutput,
    DocboxManagementInterface, DocboxServiceError, FailedTenantMigration, GetTenantInput,
    GetTenantOutput, GetTenantPendingMigrationsInput, GetTenantPendingMigrationsOutput,
    GetTenantsInput, GetTenantsOutput, ManagedTenant, ManagementError, ManagementTenantTarget,
    MigrateTenantIAMInput, MigrateTenantIAMOutput, MigrateTenantInput, MigrateTenantOutput,
    PendingMigration, SetTenantAllowedCorsOriginsInput, TenantMigrationService, async_trait,
    error::DynServiceError,
};
use thiserror::Error;

use crate::{
    config::ServerConfigData,
    server::ManagedServer,
    tenant::{
        MigrateTenantsOutcome, TenantTarget,
        create_tenant::CreateTenantConfig,
        delete_tenant::{DeleteTenant, DeleteTenantOptions},
        migrate_tenants::MigrateTenantsConfig,
        migrate_tenants_search::MigrateTenantsSearchConfig,
        migrate_tenants_storage::MigrateTenantsStorageConfig,
    },
};

pub struct ManagedServerInterface {
    pub server: ManagedServer,
    pub config: ServerConfigData,
}

#[derive(Debug, Error)]
#[error(transparent)]
pub struct ManagementDbErr(DbErr);

impl DocboxServiceError for ManagementDbErr {}

#[derive(Debug, Error)]
#[error(transparent)]
pub struct ManagementStorageLayerErr(StorageLayerError);

impl DocboxServiceError for ManagementStorageLayerErr {}

#[derive(Debug, Error)]
#[error("tenant not found")]
pub struct TenantNotFoundError;

impl DocboxServiceError for TenantNotFoundError {}

impl DocboxServiceError for crate::root::initialize::InitializeError {}

impl DocboxServiceError for crate::root::migrate_root::MigrateRootError {}

impl DocboxServiceError for crate::tenant::flush_tenant_cache::FlushTenantCacheError {}

impl DocboxServiceError for crate::tenant::delete_tenant::DeleteTenantError {}

impl DocboxServiceError for crate::tenant::create_tenant::CreateTenantError {}

impl DocboxServiceError for crate::tenant::migrate_tenants::MigrateTenantsError {}

impl DocboxServiceError for crate::tenant::migrate_tenants_search::MigrateTenantsSearchError {}

impl DocboxServiceError for crate::tenant::migrate_tenants_storage::MigrateTenantsStorageError {}

impl DocboxServiceError for crate::tenant::migrate_tenant_secret_to_iam::MigrateIAMError {}

impl DocboxServiceError
    for crate::tenant::get_pending_tenant_search_migrations::GetPendingTenantMigrationsError
{
}

impl DocboxServiceError
    for crate::tenant::get_pending_tenant_storage_migrations::GetPendingTenantMigrationsError
{
}

impl From<TenantNotFoundError> for ManagementError {
    fn from(value: TenantNotFoundError) -> Self {
        ManagementError::Service(DynServiceError::from(value))
    }
}

impl From<ManagementDbErr> for ManagementError {
    fn from(value: ManagementDbErr) -> Self {
        ManagementError::Service(DynServiceError::from(value))
    }
}

impl From<ManagementStorageLayerErr> for ManagementError {
    fn from(value: ManagementStorageLayerErr) -> Self {
        ManagementError::Service(DynServiceError::from(value))
    }
}

fn map_managed_tenant(tenant: Tenant) -> ManagedTenant {
    ManagedTenant {
        env: tenant.env,
        id: tenant.id,
        name: tenant.name,
        db_name: tenant.db_name,
        db_secret_name: tenant.db_secret_name,
        db_iam_user_name: tenant.db_iam_user_name,
        s3_name: tenant.s3_name,
        os_index_name: tenant.os_index_name,
        event_queue_url: tenant.event_queue_url,
    }
}

#[async_trait]
impl DocboxManagementInterface for ManagedServerInterface {
    async fn check_root(&self) -> Result<CheckRootOutput, ManagementError> {
        let initialized = crate::root::initialize::is_initialized(&self.server.db_provider)
            .await
            .map_err(ManagementDbErr)
            .map_err(DynServiceError::from)?;

        Ok(CheckRootOutput { initialized })
    }

    async fn create_root(&self) -> Result<(), ManagementError> {
        if self.config.database.root_iam {
            crate::root::initialize::initialize_iam(&self.server.db_provider)
                .await
                .map_err(DynServiceError::from)?;
        } else if let Some(root_secret_name) = self.config.database.root_secret_name.as_ref() {
            crate::root::initialize::initialize(
                &self.server.db_provider,
                &self.server.secrets,
                root_secret_name,
            )
            .await
            .map_err(DynServiceError::from)?;
        }

        Ok(())
    }

    async fn create_tenant(
        &self,
        input: CreateTenantInput,
    ) -> Result<CreateTenantOutput, ManagementError> {
        let tenant = crate::tenant::create_tenant::create_tenant(
            &self.server.db_provider,
            &self.server.search,
            &self.server.storage,
            &self.server.secrets,
            CreateTenantConfig {
                id: input.id,
                name: input.name,
                env: input.env,
                db_name: input.db_name,
                db_role_name: input.db_role_name,
                db_secret_name: input.db_secret_name,
                db_iam_user: input.db_iam_user,
                storage_bucket_name: input.storage_bucket_name,
                storage_cors_origins: input.storage_cors_origins,
                storage_s3_queue_arn: input.storage_s3_queue_arn,
                search_index_name: input.search_index_name,
                event_queue_url: input.event_queue_url,
            },
        )
        .await
        .map_err(DynServiceError::from)?;

        Ok(CreateTenantOutput {
            tenant: map_managed_tenant(tenant),
        })
    }

    async fn get_tenant(&self, input: GetTenantInput) -> Result<GetTenantOutput, ManagementError> {
        let tenant = crate::tenant::get_tenant::get_tenant(
            &self.server.db_provider,
            &input.env,
            input.tenant_id,
        )
        .await
        .map_err(ManagementDbErr)
        .map_err(DynServiceError::from)?;

        Ok(GetTenantOutput {
            tenant: tenant.map(map_managed_tenant),
        })
    }

    async fn delete_tenant(
        &self,
        input: DeleteTenantInput,
    ) -> Result<DeleteTenantOutput, ManagementError> {
        let tenant = crate::tenant::get_tenant::get_tenant(
            &self.server.db_provider,
            &input.env,
            input.tenant_id,
        )
        .await
        .map_err(ManagementDbErr)?
        .ok_or(TenantNotFoundError)?;

        // Must close the connections in advance to ensure the tenant
        // database can be deleted
        self.server.db_cache.close_tenant_pool(&tenant).await;

        // Tell the API server to flush and close its database pools
        self.flush_tenant_cache().await?;

        crate::tenant::delete_tenant::delete_tenant(
            &self.server.db_provider,
            &self.server.search,
            &self.server.storage,
            &self.server.events,
            &self.server.secrets,
            DeleteTenant {
                env: input.env,
                tenant_id: input.tenant_id,
                options: DeleteTenantOptions {
                    delete_contents: input.options.delete_contents,
                    delete_database: input.options.delete_database,
                    delete_search: input.options.delete_search,
                    delete_storage: input.options.delete_storage,
                    permanently_delete_secret: input.options.permanently_delete_secret,
                },
            },
        )
        .await
        .map_err(DynServiceError::from)?;

        Ok(DeleteTenantOutput {})
    }

    async fn get_tenants(
        &self,
        input: GetTenantsInput,
    ) -> Result<GetTenantsOutput, ManagementError> {
        let mut tenants = crate::tenant::get_tenants::get_tenants(&self.server.db_provider)
            .await
            .map_err(ManagementDbErr)?;

        if let Some(env) = input.env {
            tenants.retain(|tenant| tenant.env.eq(&env));
        }

        Ok(GetTenantsOutput {
            tenants: tenants.into_iter().map(map_managed_tenant).collect(),
        })
    }

    async fn set_tenant_allowed_cors_origins(
        &self,
        input: SetTenantAllowedCorsOriginsInput,
    ) -> Result<(), ManagementError> {
        let tenant = crate::tenant::get_tenant::get_tenant(
            &self.server.db_provider,
            &input.env,
            input.tenant_id,
        )
        .await
        .map_err(ManagementDbErr)?
        .ok_or(TenantNotFoundError)?;

        let storage = self
            .server
            .storage
            .create_layer(tenant.storage_layer_options());

        storage
            .set_bucket_cors_origins(input.origins)
            .await
            .map_err(|err| DynServiceError::from(ManagementStorageLayerErr(err)))?;

        Ok(())
    }

    async fn migrate_root(&self) -> Result<(), ManagementError> {
        crate::root::migrate_root::migrate_root(&self.server.db_provider, None)
            .await
            .map_err(DynServiceError::from)?;

        Ok(())
    }

    async fn migrate_tenant(
        &self,
        input: MigrateTenantInput,
    ) -> Result<MigrateTenantOutput, ManagementError> {
        let target = ApplyServiceMigrationTarget {
            env: input.env,
            tenant_id: input.tenant_id,
            name: input.name,
            skip_failed: input.skip_failed,
        };

        if let Some(service) = input.service {
            return self.apply_service_migration(service, &target).await;
        }

        let database_outcome = self
            .apply_service_migration(TenantMigrationService::Database, &target)
            .await?;

        let search_outcome = self
            .apply_service_migration(TenantMigrationService::Search, &target)
            .await?;

        let storage_outcome = self
            .apply_service_migration(TenantMigrationService::Storage, &target)
            .await?;

        let applied_tenants: Vec<ManagementTenantTarget> = database_outcome
            .applied_tenants
            .into_iter()
            .chain(search_outcome.applied_tenants)
            .chain(storage_outcome.applied_tenants)
            .collect();

        let failed_tenants: Vec<FailedTenantMigration> = database_outcome
            .failed_tenants
            .into_iter()
            .chain(search_outcome.failed_tenants)
            .chain(storage_outcome.failed_tenants)
            .collect();

        Ok(MigrateTenantOutput {
            applied_tenants,
            failed_tenants,
        })
    }

    async fn migrate_tenant_iam(
        &self,
        input: MigrateTenantIAMInput,
    ) -> Result<MigrateTenantIAMOutput, ManagementError> {
        let mut tenants = crate::tenant::get_tenants::get_tenants(&self.server.db_provider)
            .await
            .map_err(ManagementDbErr)?;

        tenants.retain(|tenant| {
            tenant.env.eq(&input.env) && input.tenant_id.is_none_or(|id| tenant.id.eq(&id))
        });

        let mut migrated_tenants = Vec::new();

        for mut tenant in tenants {
            if tenant.db_iam_user_name.is_some() {
                tracing::debug!(?tenant, "skipping tenant with iam user name already set");
                continue;
            }

            crate::tenant::migrate_tenant_secret_to_iam::migrate_tenant_secret_to_iam(
                &self.server.db_provider,
                &self.server.secrets,
                &mut tenant,
            )
            .await
            .map_err(DynServiceError::from)?;

            migrated_tenants.push(ManagementTenantTarget {
                id: tenant.id,
                name: tenant.name,
                env: tenant.env,
            });
        }

        Ok(MigrateTenantIAMOutput {
            applied_tenants: migrated_tenants,
        })
    }

    async fn get_pending_root_migrations(&self) -> Result<Vec<String>, ManagementError> {
        let pending_migrations =
            crate::root::get_pending_root_migrations::get_pending_root_migrations(
                &self.server.db_provider,
            )
            .await
            .map_err(ManagementDbErr)?;
        Ok(pending_migrations)
    }

    async fn get_tenant_pending_migrations(
        &self,
        input: GetTenantPendingMigrationsInput,
    ) -> Result<GetTenantPendingMigrationsOutput, ManagementError> {
        let tenant = crate::tenant::get_tenant::get_tenant(
            &self.server.db_provider,
            &input.env,
            input.tenant_id,
        )
        .await
        .map_err(ManagementDbErr)?
        .ok_or(TenantNotFoundError)?;

        if let Some(service) = input.service {
            let migrations = self.get_service_migrations(service, &tenant).await?;
            return Ok(GetTenantPendingMigrationsOutput { migrations });
        }

        let database_migrations = self
            .get_service_migrations(TenantMigrationService::Database, &tenant)
            .await?;
        let search_migrations = self
            .get_service_migrations(TenantMigrationService::Search, &tenant)
            .await?;
        let storage_migrations = self
            .get_service_migrations(TenantMigrationService::Storage, &tenant)
            .await?;

        let migrations = database_migrations
            .into_iter()
            .chain(search_migrations)
            .chain(storage_migrations)
            .collect();

        Ok(GetTenantPendingMigrationsOutput { migrations })
    }

    async fn flush_tenant_cache(&self) -> Result<(), ManagementError> {
        crate::tenant::flush_tenant_cache::flush_tenant_cache(&self.config.api)
            .await
            .map_err(DynServiceError::from)?;

        Ok(())
    }
}

struct ApplyServiceMigrationTarget {
    env: Option<String>,
    tenant_id: Option<Uuid>,
    name: Option<String>,
    skip_failed: bool,
}

fn map_tenant_target(tenant: TenantTarget) -> ManagementTenantTarget {
    ManagementTenantTarget {
        env: tenant.env,
        id: tenant.tenant_id,
        name: tenant.name,
    }
}

fn map_migrate_tenants_outcome(outcome: MigrateTenantsOutcome) -> MigrateTenantOutput {
    MigrateTenantOutput {
        applied_tenants: outcome
            .applied_tenants
            .into_iter()
            .map(map_tenant_target)
            .collect(),
        failed_tenants: outcome
            .failed_tenants
            .into_iter()
            .map(|(error, target)| FailedTenantMigration {
                error,
                target: map_tenant_target(target),
            })
            .collect(),
    }
}

impl ManagedServerInterface {
    async fn apply_service_migration(
        &self,
        service: TenantMigrationService,
        target: &ApplyServiceMigrationTarget,
    ) -> Result<MigrateTenantOutput, ManagementError> {
        match service {
            TenantMigrationService::Database => {
                let outcome = crate::tenant::migrate_tenants::migrate_tenants(
                    &self.server.db_provider,
                    MigrateTenantsConfig {
                        env: target.env.clone(),
                        tenant_id: target.tenant_id,
                        skip_failed: target.skip_failed,
                        target_migration_name: target.name.clone(),
                    },
                )
                .await
                .map_err(DynServiceError::from)?;

                Ok(map_migrate_tenants_outcome(outcome))
            }
            TenantMigrationService::Search => {
                let outcome = crate::tenant::migrate_tenants_search::migrate_tenants_search(
                    &self.server.db_provider,
                    &self.server.search,
                    MigrateTenantsSearchConfig {
                        env: target.env.clone(),
                        tenant_id: target.tenant_id,
                        skip_failed: target.skip_failed,
                        target_migration_name: target.name.clone(),
                    },
                )
                .await
                .map_err(DynServiceError::from)?;

                Ok(map_migrate_tenants_outcome(outcome))
            }
            TenantMigrationService::Storage => {
                let outcome = crate::tenant::migrate_tenants_storage::migrate_tenants_storage(
                    &self.server.db_provider,
                    &self.server.storage,
                    MigrateTenantsStorageConfig {
                        env: target.env.clone(),
                        tenant_id: target.tenant_id,
                        skip_failed: target.skip_failed,
                        target_migration_name: target.name.clone(),
                    },
                )
                .await
                .map_err(DynServiceError::from)?;

                Ok(map_migrate_tenants_outcome(outcome))
            }
        }
    }

    async fn get_service_migrations(
        &self,
        service: TenantMigrationService,
        tenant: &Tenant,
    ) -> Result<Vec<PendingMigration>, ManagementError> {
        match service {
            TenantMigrationService::Database => {
                let pending_migrations =
                    crate::tenant::get_pending_tenant_migrations::get_pending_tenant_migrations(
                        &self.server.db_provider,
                        tenant,
                    )
                    .await
                    .map_err(ManagementDbErr)?;

                Ok(pending_migrations
                    .into_iter()
                    .map(|name| PendingMigration { name, service })
                    .collect())
            }
            TenantMigrationService::Search => {
                let pending_migrations = crate::tenant::get_pending_tenant_storage_migrations::get_pending_tenant_storage_migrations(
                    &self.server.db_provider,
                    &self.server.storage,
                    tenant,
                )
                .await
                .map_err(DynServiceError::from)?;

                Ok(pending_migrations
                    .into_iter()
                    .map(|name| PendingMigration { name, service })
                    .collect())
            }
            TenantMigrationService::Storage => {
                let pending_migrations = crate::tenant::get_pending_tenant_search_migrations::get_pending_tenant_search_migrations(
                    &self.server.db_provider,
                    &self.server.search,
                    tenant,
                )
                .await    .map_err(DynServiceError::from)?;

                Ok(pending_migrations
                    .into_iter()
                    .map(|name| PendingMigration { name, service })
                    .collect())
            }
        }
    }
}
