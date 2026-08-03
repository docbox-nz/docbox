use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManagedTenant {
    /// Environment for the tenant
    pub env: String,
    /// Unique ID for the tenant
    pub id: Uuid,
    /// Name for the tenant
    pub name: String,
    /// Name of the tenant database
    pub db_name: String,
    /// Name for the AWS secret used for the database user if
    /// using secret based authentication
    pub db_secret_name: Option<String>,
    /// Name for the database user username if using IAM based
    /// authentication
    pub db_iam_user_name: Option<String>,
    /// Name of the tenant s3 bucket
    pub s3_name: String,
    /// Name of the tenant search index
    pub os_index_name: String,
    /// Optional event queue (SQS) to send docbox events to
    pub event_queue_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManagementTenantTarget {
    /// Unique ID for the tenant
    pub id: Uuid,
    /// Name for the tenant
    pub name: String,
    /// Environment for the tenant
    pub env: String,
}
