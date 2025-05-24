use crate::error::HttpError;
use axum::http::StatusCode;
use docbox_database::models::{document_box::DocumentBox, tenant::Tenant};
use serde::Serialize;
use thiserror::Error;

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
