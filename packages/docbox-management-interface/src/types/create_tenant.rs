use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::tenant::ManagedTenant;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateTenantInput {
    /// Unique ID for the tenant
    pub id: Uuid,
    /// Name of the tenant
    pub name: String,
    /// Environment for the tenant
    pub env: String,

    /// Database name for the tenant
    pub db_name: String,
    /// Name for the tenant role
    pub db_role_name: String,

    /// Database secret credentials name for the tenant
    /// (Where the username and password will be stored)
    #[serde(default)]
    pub db_secret_name: Option<String>,

    /// Whether to use IAM for role authorization instead
    /// of a database secret
    #[serde(default)]
    pub db_iam_user: bool,

    /// Name of the tenant storage bucket
    pub storage_bucket_name: String,
    /// CORS Origins for setting up presigned uploads with S3
    #[serde(default)]
    pub storage_cors_origins: Vec<String>,
    /// ARN for the S3 queue to publish S3 notifications, required
    /// for presigned uploads
    #[serde(default)]
    pub storage_s3_queue_arn: Option<String>,

    /// Name of the tenant search index
    pub search_index_name: String,

    /// URL for the SQS event queue
    #[serde(default)]
    pub event_queue_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateTenantOutput {
    /// The created tenant
    pub tenant: ManagedTenant,
}
