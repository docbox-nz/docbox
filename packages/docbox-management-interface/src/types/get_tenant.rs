use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::tenant::ManagedTenant;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetTenantInput {
    /// Environment of the tenant to find
    pub env: String,
    /// The ID of the tenant
    pub tenant_id: Uuid,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetTenantOutput {
    pub tenant: Option<ManagedTenant>,
}
