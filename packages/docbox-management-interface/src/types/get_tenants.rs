use serde::{Deserialize, Serialize};

use crate::tenant::ManagedTenant;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetTenantsInput {
    /// Optional filter for the tenants environment
    pub env: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetTenantsOutput {
    pub tenants: Vec<ManagedTenant>,
}
