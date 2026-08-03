use async_trait::async_trait;
use serde::de::DeserializeOwned;

use crate::{
    CheckRootOutput, CreateTenantInput, CreateTenantOutput, DeleteTenantInput, DeleteTenantOutput,
    DocboxManagementCommand, DocboxManagementInterface, GetTenantInput, GetTenantOutput,
    GetTenantPendingMigrationsInput, GetTenantPendingMigrationsOutput, GetTenantsInput,
    GetTenantsOutput, ManagementError, MigrateTenantIAMInput, MigrateTenantIAMOutput,
    MigrateTenantInput, MigrateTenantOutput, SetTenantAllowedCorsOriginsInput,
};

/// Docbox management interface that is accessed through some
/// underlying transport that sends docbox management commands
///
/// Used for remote management i.e management lambda
pub struct RemoteDocboxManagementInterface<Transport> {
    /// The transport that sends commands and receives responses
    transport: Transport,
}

impl<Transport> RemoteDocboxManagementInterface<Transport> {
    pub fn new(transport: Transport) -> Self {
        Self { transport }
    }
}

#[async_trait]
pub trait RemoteDocboxManagementTransport: Send + Sync + 'static {
    async fn execute_command<T>(
        &self,
        command: DocboxManagementCommand,
    ) -> Result<T, ManagementError>
    where
        T: DeserializeOwned;
}

#[async_trait]
impl<Core: RemoteDocboxManagementTransport> DocboxManagementInterface
    for RemoteDocboxManagementInterface<Core>
{
    async fn check_root(&self) -> Result<CheckRootOutput, ManagementError> {
        self.transport
            .execute_command(DocboxManagementCommand::CheckRoot)
            .await
    }

    async fn create_root(&self) -> Result<(), ManagementError> {
        self.transport
            .execute_command(DocboxManagementCommand::CreateRoot)
            .await
    }

    async fn create_tenant(
        &self,
        input: CreateTenantInput,
    ) -> Result<CreateTenantOutput, ManagementError> {
        self.transport
            .execute_command(DocboxManagementCommand::CreateTenant(input))
            .await
    }

    async fn get_tenant(&self, input: GetTenantInput) -> Result<GetTenantOutput, ManagementError> {
        self.transport
            .execute_command(DocboxManagementCommand::GetTenant(input))
            .await
    }

    async fn delete_tenant(
        &self,
        input: DeleteTenantInput,
    ) -> Result<DeleteTenantOutput, ManagementError> {
        self.transport
            .execute_command(DocboxManagementCommand::DeleteTenant(input))
            .await
    }

    async fn get_tenants(
        &self,
        input: GetTenantsInput,
    ) -> Result<GetTenantsOutput, ManagementError> {
        self.transport
            .execute_command(DocboxManagementCommand::GetTenants(input))
            .await
    }

    async fn set_tenant_allowed_cors_origins(
        &self,
        input: SetTenantAllowedCorsOriginsInput,
    ) -> Result<(), ManagementError> {
        self.transport
            .execute_command(DocboxManagementCommand::SetTenantAllowedCorsOrigins(input))
            .await
    }

    async fn migrate_root(&self) -> Result<(), ManagementError> {
        self.transport
            .execute_command(DocboxManagementCommand::MigrateRoot)
            .await
    }

    async fn migrate_tenant(
        &self,
        input: MigrateTenantInput,
    ) -> Result<MigrateTenantOutput, ManagementError> {
        self.transport
            .execute_command(DocboxManagementCommand::MigrateTenant(input))
            .await
    }

    async fn migrate_tenant_iam(
        &self,
        input: MigrateTenantIAMInput,
    ) -> Result<MigrateTenantIAMOutput, ManagementError> {
        self.transport
            .execute_command(DocboxManagementCommand::MigrateIAM(input))
            .await
    }

    async fn get_pending_root_migrations(&self) -> Result<Vec<String>, ManagementError> {
        self.transport
            .execute_command(DocboxManagementCommand::GetPendingRootMigrations)
            .await
    }

    async fn get_tenant_pending_migrations(
        &self,
        input: GetTenantPendingMigrationsInput,
    ) -> Result<GetTenantPendingMigrationsOutput, ManagementError> {
        self.transport
            .execute_command(DocboxManagementCommand::GetTenantPendingMigrations(input))
            .await
    }

    async fn flush_tenant_cache(&self) -> Result<(), ManagementError> {
        self.transport
            .execute_command(DocboxManagementCommand::FlushTenantCache)
            .await
    }
}
