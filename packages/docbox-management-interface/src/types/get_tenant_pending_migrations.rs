use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{PendingMigration, TenantMigrationService};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetTenantPendingMigrationsInput {
    pub env: String,
    pub tenant_id: Uuid,
    /// Which service to get migrations for omit to obtain for all services
    #[serde(default)]
    pub service: Option<TenantMigrationService>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetTenantPendingMigrationsOutput {
    /// List of pending migrations
    pub migrations: Vec<PendingMigration>,
}
