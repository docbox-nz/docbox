use crate::error::HttpError;
use axum::http::StatusCode;
use docbox_database::models::{
    document_box::DocumentBox,
    folder::{FolderWithExtra, ResolvedFolderWithExtra},
};
use garde::Validate;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Request to create a document box
#[derive(Debug, Validate, Deserialize)]
pub struct CreateDocumentBox {
    /// The document box scope
    #[garde(length(min = 1))]
    pub scope: String,
}

/// Response to an options request
#[derive(Debug, Serialize)]
pub struct DocumentBoxOptions {
    /// List of allowed mime types for uploading
    pub allowed_mime_types: &'static [&'static str],
    /// Max allowed upload file size in bytes
    pub max_file_size: usize,
}

/// Response for requesting a document box
#[derive(Debug, Serialize)]
pub struct DocumentBoxResponse {
    /// The created document box
    pub document_box: DocumentBox,
    /// Root folder of the document box
    pub root: FolderWithExtra,
    /// Resolved contents of the root folder
    pub children: ResolvedFolderWithExtra,
}

#[derive(Debug, Serialize)]
pub struct DocumentBoxStats {
    /// Total number of files within the document box
    pub total_files: i64,
    /// Total number of links within the document box
    pub total_links: i64,
    /// Total number of folders within the document box
    pub total_folders: i64,
}

#[derive(Debug, Error)]
pub enum DocumentBoxError {
    #[error("document box with matching scope already exists")]
    ScopeAlreadyExists,

    #[error("unknown document box")]
    UnknownDocumentBox,

    #[error("document box missing root folder")]
    MissingDocumentBoxRoot,
}

impl HttpError for DocumentBoxError {
    fn log(&self) {}

    fn status(&self) -> axum::http::StatusCode {
        match self {
            DocumentBoxError::ScopeAlreadyExists => StatusCode::CONFLICT,
            DocumentBoxError::UnknownDocumentBox => StatusCode::NOT_FOUND,
            DocumentBoxError::MissingDocumentBoxRoot => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}
