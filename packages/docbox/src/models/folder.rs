use crate::error::HttpError;
use axum::http::StatusCode;
use docbox_core::folders::create_folder::CreateFolderError;
use docbox_database::models::folder::{FolderId, FolderWithExtra, ResolvedFolderWithExtra};
use garde::Validate;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use utoipa::ToSchema;

/// Request to create a folder
#[derive(Debug, Validate, Deserialize, ToSchema)]
pub struct CreateFolderRequest {
    /// Name for the folder
    #[garde(length(min = 1, max = 255))]
    #[schema(min_length = 1, max_length = 255)]
    pub name: String,

    /// Folder to store folder in
    #[garde(skip)]
    #[schema(value_type = Uuid)]
    pub folder_id: FolderId,
}

/// Response for requesting a document box
#[derive(Debug, Serialize, ToSchema)]
pub struct FolderResponse {
    /// The folder itself
    pub folder: FolderWithExtra,

    /// Resolved contents of the folder
    pub children: ResolvedFolderWithExtra,
}

/// Request to rename and or move a folder
#[derive(Debug, Validate, Deserialize, ToSchema)]
pub struct UpdateFolderRequest {
    /// Name for the folder
    #[garde(inner(length(min = 1, max = 255)))]
    #[schema(min_length = 1, max_length = 255)]
    pub name: Option<String>,

    /// New parent folder for the folder
    #[garde(skip)]
    #[schema(value_type = Option<Uuid>)]
    pub folder_id: Option<FolderId>,

    /// Whether to pin the folder
    #[garde(skip)]
    #[schema(value_type = Option<bool>)]
    pub pinned: Option<bool>,
}

#[derive(Debug, Error)]
pub enum HttpFolderError {
    #[error("unknown folder")]
    UnknownFolder,

    /// Failed to create the folder
    #[error(transparent)]
    CreateError(CreateFolderError),

    #[error("unknown target folder")]
    UnknownTargetFolder,

    #[error("cannot delete root folder")]
    CannotDeleteRoot,

    #[error("cannot modify root folder")]
    CannotModifyRoot,

    #[error("cannot move a folder into itself")]
    CannotMoveIntoSelf,
}

impl HttpError for HttpFolderError {
    fn status(&self) -> axum::http::StatusCode {
        match self {
            HttpFolderError::UnknownFolder | HttpFolderError::UnknownTargetFolder => {
                StatusCode::NOT_FOUND
            }
            HttpFolderError::CannotModifyRoot
            | HttpFolderError::CannotDeleteRoot
            | HttpFolderError::CannotMoveIntoSelf => StatusCode::BAD_REQUEST,
            HttpFolderError::CreateError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}
