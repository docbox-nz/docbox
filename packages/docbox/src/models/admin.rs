use crate::error::HttpError;
use axum::http::StatusCode;
use docbox_database::models::{
    document_box::DocumentBox,
    tenant::{Tenant, TenantId},
};
use garde::Validate;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Request to create a tenant
#[derive(Debug, Validate, Deserialize)]
pub struct CreateTenant {
    /// Unique ID for the tenant
    #[garde(skip)]
    pub id: TenantId,

    /// Database name for the tenant
    #[garde(length(min = 1))]
    pub db_name: String,

    /// Database secret credentials name for the tenant
    #[garde(length(min = 1))]
    pub db_secret_name: String,

    /// Name of the tenant s3 bucket
    #[garde(length(min = 1))]
    pub s3_name: String,

    /// Name of the tenant search index
    #[garde(length(min = 1))]
    pub os_index_name: String,

    /// URL for the SQS event queue
    #[garde(inner(length(min = 1)))]
    pub event_queue_url: Option<String>,

    /// CORS Origins for setting up presigned uploads with S3
    #[garde(skip)]
    pub origins: Vec<String>,

    /// ARN for the S3 queue to publish S3 notifications, required
    /// for presigned uploads
    #[garde(skip)]
    pub s3_queue_arn: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreatedTenant {
    /// ID of the created tenant
    pub id: TenantId,
}

#[derive(Debug, Serialize)]
pub struct TenantResponse {
    pub tenant: Tenant,
    pub document_boxes: Vec<DocumentBox>,
}

#[derive(Debug, Error)]
pub enum HttpTenantError {
    #[error("unknown tenant")]
    UnknownTenant,
}

impl HttpError for HttpTenantError {
    fn log(&self) {}

    fn status(&self) -> axum::http::StatusCode {
        match self {
            HttpTenantError::UnknownTenant => StatusCode::NOT_FOUND,
        }
    }
}
