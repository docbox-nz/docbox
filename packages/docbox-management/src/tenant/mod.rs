use docbox_database::models::tenant::TenantId;
use serde::{Deserialize, Serialize};

pub mod create_tenant;
pub mod delete_tenant;
pub mod get_pending_tenant_migrations;
pub mod get_pending_tenant_search_migrations;
pub mod get_tenant;
pub mod get_tenants;
pub mod migrate_tenant;
pub mod migrate_tenant_search;
pub mod migrate_tenants;
pub mod migrate_tenants_search;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TenantTarget {
    pub env: String,
    pub name: String,
    pub tenant_id: TenantId,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MigrateTenantsOutcome {
    pub applied_tenants: Vec<TenantTarget>,
    pub failed_tenants: Vec<(String, TenantTarget)>,
}
