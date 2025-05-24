use crate::error::HttpError;
use axum::http::StatusCode;
use docbox_database::models::{
    document_box::DocumentBox,
    folder::{FolderWithExtra, ResolvedFolderWithExtra},
};
use garde::Validate;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use utoipa::ToSchema;

/// Request to create a document box
#[derive(Debug, Validate, Deserialize, ToSchema)]
pub struct CreateDocumentBoxRequest {
    /// Scope for the document box to use
    #[garde(length(min = 1))]
    #[schema(min_length = 1)]
    pub scope: String,
}

/// Response to an options request
#[derive(Debug, Serialize, ToSchema)]
pub struct DocumentBoxOptions {
    /// List of allowed mime types for uploading
    pub allowed_mime_types: &'static [&'static str],
    /// Max allowed upload file size in bytes
    pub max_file_size: usize,
}

/// Response for requesting a document box
#[derive(Debug, Serialize, ToSchema)]
pub struct DocumentBoxResponse {
    /// The created document box
    pub document_box: DocumentBox,
    /// Root folder of the document box
    pub root: FolderWithExtra,
    /// Resolved contents of the root folder
    pub children: ResolvedFolderWithExtra,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DocumentBoxStats {
    /// Total number of files within the document box
    pub total_files: i64,
    /// Total number of links within the document box
    pub total_links: i64,
    /// Total number of folders within the document box
    pub total_folders: i64,
}

#[derive(Debug, Error)]
pub enum HttpDocumentBoxError {
    #[error("document box with matching scope already exists")]
    ScopeAlreadyExists,

    #[error("unknown document box")]
    UnknownDocumentBox,
}

impl HttpError for HttpDocumentBoxError {
    fn status(&self) -> axum::http::StatusCode {
        match self {
            HttpDocumentBoxError::ScopeAlreadyExists => StatusCode::CONFLICT,
            HttpDocumentBoxError::UnknownDocumentBox => StatusCode::NOT_FOUND,
        }
    }
}
