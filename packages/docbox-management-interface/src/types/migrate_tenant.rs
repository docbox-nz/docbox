use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{ManagementTenantTarget, TenantMigrationService};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MigrateTenantInput {
    /// Filter for the environment to apply the migration within
    pub env: Option<String>,
    /// Filter to only apply the migration to a specific tenant
    pub tenant_id: Option<Uuid>,
    /// Filter for a specific service to apply migrations for
    pub service: Option<TenantMigrationService>,
    /// Filter for a specific migration to run
    pub name: Option<String>,
    /// Filter to skip failed migrations and continue
    pub skip_failed: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MigrateTenantOutput {
    /// Tenants where migrations were successfully applied
    pub applied_tenants: Vec<ManagementTenantTarget>,
    /// Tenants where the migrations failed to apply
    pub failed_tenants: Vec<FailedTenantMigration>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FailedTenantMigration {
    pub target: ManagementTenantTarget,
    pub error: String,
}
