pub mod error;
pub mod remote;
pub mod types;

pub use async_trait::async_trait;
pub use error::{DocboxServiceError, ManagementError};
pub use remote::{RemoteDocboxManagementInterface, RemoteDocboxManagementTransport};
use serde::{Deserialize, Serialize};
pub use types::*;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "command", content = "payload")]
pub enum DocboxManagementCommand {
    CreateRoot,
    CheckRoot,
    CreateTenant(CreateTenantInput),
    GetTenant(GetTenantInput),
    DeleteTenant(DeleteTenantInput),
    GetTenants(GetTenantsInput),
    SetTenantAllowedCorsOrigins(SetTenantAllowedCorsOriginsInput),
    MigrateRoot,
    MigrateTenant(MigrateTenantInput),
    MigrateIAM(MigrateTenantIAMInput),
    GetPendingRootMigrations,
    GetTenantPendingMigrations(GetTenantPendingMigrationsInput),
    FlushTenantCache,
}

fn serialize_value<T: Serialize>(value: T) -> Result<serde_json::Value, ManagementError> {
    serde_json::to_value(value).map_err(ManagementError::SerializeResponse)
}

impl DocboxManagementCommand {
    pub async fn execute(
        self,
        interface: &dyn DocboxManagementInterface,
    ) -> Result<serde_json::Value, ManagementError> {
        match self {
            DocboxManagementCommand::CreateRoot => serialize_value(interface.create_root().await?),
            DocboxManagementCommand::CheckRoot => serialize_value(interface.check_root().await?),
            DocboxManagementCommand::CreateTenant(input) => {
                serialize_value(interface.create_tenant(input).await?)
            }
            DocboxManagementCommand::GetTenant(input) => {
                serialize_value(interface.get_tenant(input).await?)
            }
            DocboxManagementCommand::DeleteTenant(input) => {
                serialize_value(interface.delete_tenant(input).await?)
            }
            DocboxManagementCommand::GetTenants(input) => {
                serialize_value(interface.get_tenants(input).await?)
            }
            DocboxManagementCommand::SetTenantAllowedCorsOrigins(input) => {
                serialize_value(interface.set_tenant_allowed_cors_origins(input).await?)
            }
            DocboxManagementCommand::MigrateRoot => {
                serialize_value(interface.migrate_root().await?)
            }
            DocboxManagementCommand::MigrateTenant(input) => {
                serialize_value(interface.migrate_tenant(input).await?)
            }
            DocboxManagementCommand::MigrateIAM(input) => {
                serialize_value(interface.migrate_tenant_iam(input).await?)
            }
            DocboxManagementCommand::GetPendingRootMigrations => {
                serialize_value(interface.get_pending_root_migrations().await?)
            }
            DocboxManagementCommand::GetTenantPendingMigrations(input) => {
                serialize_value(interface.get_tenant_pending_migrations(input).await?)
            }
            DocboxManagementCommand::FlushTenantCache => {
                serialize_value(interface.flush_tenant_cache().await?)
            }
        }
    }
}

/// Management interface providing the management functionality with an abstracted backend
/// to allow the various points of management (CLI, Management Lambda, ..etc)
#[async_trait::async_trait]
pub trait DocboxManagementInterface {
    /// Checks if the docbox root database has been initialized
    async fn check_root(&self) -> Result<CheckRootOutput, ManagementError>;

    /// Create the root docbox database
    async fn create_root(&self) -> Result<(), ManagementError>;

    /// Create a new tenant
    async fn create_tenant(
        &self,
        input: CreateTenantInput,
    ) -> Result<CreateTenantOutput, ManagementError>;

    /// Get a specific tenant
    async fn get_tenant(&self, input: GetTenantInput) -> Result<GetTenantOutput, ManagementError>;

    /// Delete a specific tenant
    async fn delete_tenant(
        &self,
        input: DeleteTenantInput,
    ) -> Result<DeleteTenantOutput, ManagementError>;

    /// Get a collection of tenants
    async fn get_tenants(
        &self,
        input: GetTenantsInput,
    ) -> Result<GetTenantsOutput, ManagementError>;

    /// Set the allowed CORS origins for a tenants storage bucket
    async fn set_tenant_allowed_cors_origins(
        &self,
        input: SetTenantAllowedCorsOriginsInput,
    ) -> Result<(), ManagementError>;

    /// Apply root database migrations
    async fn migrate_root(&self) -> Result<(), ManagementError>;

    /// Apply migrations for tenant(s)
    async fn migrate_tenant(
        &self,
        input: MigrateTenantInput,
    ) -> Result<MigrateTenantOutput, ManagementError>;

    /// Migrate a tenant from secrets based authentication to IAM based
    /// database authentication
    async fn migrate_tenant_iam(
        &self,
        input: MigrateTenantIAMInput,
    ) -> Result<MigrateTenantIAMOutput, ManagementError>;

    /// Get migrations that are waiting to be applied to the root
    async fn get_pending_root_migrations(&self) -> Result<Vec<String>, ManagementError>;

    /// Get pending database migrations for a specific tenant
    async fn get_tenant_pending_migrations(
        &self,
        input: GetTenantPendingMigrationsInput,
    ) -> Result<GetTenantPendingMigrationsOutput, ManagementError>;

    /// Flush the tenant database cache for persisted servers
    async fn flush_tenant_cache(&self) -> Result<(), ManagementError>;
}
