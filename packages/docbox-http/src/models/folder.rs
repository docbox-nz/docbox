use crate::error::HttpError;
use axum::http::StatusCode;
use docbox_core::{
    database::models::folder::{FolderId, FolderWithExtra, ResolvedFolderWithExtra},
    folders::create_folder::CreateFolderError,
};
use garde::Validate;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use utoipa::ToSchema;
use uuid::Uuid;

/// Request to create a folder
#[derive(Debug, Validate, Deserialize, ToSchema)]
pub struct CreateFolderRequest {
    /// Name for the folder
    #[garde(length(min = 1, max = 255))]
    #[schema(min_length = 1, max_length = 255)]
    pub name: String,

    /// ID of the folder to store folder in
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

    /// ID of the new parent folder for the folder
    #[garde(skip)]
    #[schema(value_type = Option<Uuid>)]
    pub folder_id: Option<FolderId>,

    /// Whether to pin the folder
    #[garde(skip)]
    #[schema(value_type = Option<bool>)]
    pub pinned: Option<bool>,
}

/// Request to create a zip file of folder contents
#[derive(Debug, Validate, Deserialize, ToSchema)]
pub struct ZipFolderRequest {
    /// Optionally only include the specified items (files and folders)
    ///
    /// Inclusion is only applied to the direct descendants
    /// of the folder use exclude to exclude specific content
    /// from nested folders
    #[garde(skip)]
    #[serde(alias = "include_files")]
    pub include: Option<Vec<Uuid>>,

    /// Optionally exclude the specified items (files and folders)
    /// including any items that don't match the provided list of IDs
    ///
    /// Exclusion is applied deeply to nested files and folders
    #[garde(skip)]
    #[serde(alias = "exclude_files")]
    pub exclude: Option<Vec<Uuid>>,
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

    #[error("failed to create zip file")]
    CreateZipFile,
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
            HttpFolderError::CreateError(_) | HttpFolderError::CreateZipFile => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }
}
