use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ManagementTenantTarget;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MigrateTenantIAMInput {
    /// Environment to migrate tenants within
    pub env: String,
    /// Optional tenant ID to migrate a specific tenant
    pub tenant_id: Option<Uuid>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MigrateTenantIAMOutput {
    pub applied_tenants: Vec<ManagementTenantTarget>,
}
