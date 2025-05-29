use crate::error::HttpError;
use axum::http::StatusCode;
use docbox_core::links::create_link::CreateLinkError;
use docbox_database::models::folder::FolderId;
use garde::Validate;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use utoipa::ToSchema;

/// Request to create a document box
#[derive(Debug, Validate, Deserialize, ToSchema)]
pub struct CreateLink {
    /// Name for the link
    #[garde(length(min = 1))]
    #[schema(min_length = 1)]
    pub name: String,

    /// Link URL
    #[garde(length(min = 1))]
    #[schema(min_length = 1)]
    pub value: String,

    /// Folder to store link in
    #[garde(skip)]
    #[schema(value_type = Uuid)]
    pub folder_id: FolderId,
}

/// Request to rename a file
#[derive(Debug, Validate, Deserialize, ToSchema)]
pub struct UpdateLinkRequest {
    /// Name for the link
    #[garde(inner(length(min = 1)))]
    #[schema(min_length = 1)]
    pub name: Option<String>,

    /// Value for the link
    #[garde(inner(length(min = 1)))]
    #[schema(min_length = 1)]
    pub value: Option<String>,

    /// New parent folder for the link
    #[garde(skip)]
    #[schema(value_type = Option<Uuid>)]
    pub folder_id: Option<FolderId>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LinkMetadataResponse {
    pub title: Option<String>,
    pub og_title: Option<String>,
    pub og_description: Option<String>,

    pub favicon: bool,
    pub image: bool,
}

#[derive(Debug, Error)]
pub enum HttpLinkError {
    #[error("unknown link")]
    UnknownLink,

    #[error("invalid link url")]
    InvalidLinkUrl,

    /// Failed to create the link
    #[error(transparent)]
    CreateError(CreateLinkError),

    // Failed resolving of metadata is treated as a 404 as we were
    // unable to find the website data
    #[error("failed to resolve metadata")]
    FailedResolve,

    #[error("website favicon not present")]
    NoFavicon,

    #[error("website image not present")]
    NoImage,
}

impl HttpError for HttpLinkError {
    fn status(&self) -> axum::http::StatusCode {
        match self {
            HttpLinkError::UnknownLink
            | HttpLinkError::NoFavicon
            | HttpLinkError::NoImage
            | HttpLinkError::FailedResolve => StatusCode::NOT_FOUND,
            HttpLinkError::InvalidLinkUrl => StatusCode::BAD_REQUEST,
            HttpLinkError::CreateError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}
