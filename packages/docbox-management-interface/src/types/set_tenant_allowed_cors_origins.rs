use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SetTenantAllowedCorsOriginsInput {
    /// Environment of the tenant to find
    pub env: String,
    /// The ID of the tenant
    pub tenant_id: Uuid,

    /// The list of allowed CORS origins
    pub origins: Vec<String>,
}
