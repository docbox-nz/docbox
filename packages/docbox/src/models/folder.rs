use crate::error::HttpError;
use axum::http::StatusCode;
use docbox_database::models::folder::{FolderId, FolderWithExtra, ResolvedFolderWithExtra};
use garde::Validate;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Request to create a folder
#[derive(Debug, Validate, Deserialize)]
pub struct CreateFolderRequest {
    /// Name for the folder
    #[garde(length(min = 1))]
    pub name: String,

    /// Folder to store folder in
    #[garde(skip)]
    pub folder_id: FolderId,
}

/// Response for requesting a document box
#[derive(Debug, Serialize)]
pub struct FolderResponse {
    /// The folder itself
    pub folder: FolderWithExtra,

    /// Resolved contents of the folder
    pub children: ResolvedFolderWithExtra,
}

/// Request to rename and or move a folder
#[derive(Debug, Validate, Deserialize)]
pub struct UpdateFolderRequest {
    /// Name for the folder
    #[garde(inner(length(min = 1)))]
    pub name: Option<String>,

    /// New parent folder for the folder
    #[garde(skip)]
    pub folder_id: Option<FolderId>,
}

#[derive(Debug, Error)]
pub enum HttpFolderError {
    #[error("unknown folder")]
    UnknownFolder,

    #[error("unknown target folder")]
    UnknownTargetFolder,

    #[error("cannot modify root folder")]
    CannotModifyRoot,

    #[error("cannot move a folder into itself")]
    CannotMoveIntoSelf,
}

impl HttpError for HttpFolderError {
    fn log(&self) {}

    fn status(&self) -> axum::http::StatusCode {
        match self {
            HttpFolderError::UnknownFolder | HttpFolderError::UnknownTargetFolder => {
                StatusCode::NOT_FOUND
            }
            HttpFolderError::CannotModifyRoot | HttpFolderError::CannotMoveIntoSelf => {
                StatusCode::BAD_REQUEST
            }
        }
    }
}
