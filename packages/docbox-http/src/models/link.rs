use crate::error::HttpError;
use axum::http::StatusCode;
use docbox_core::{database::models::folder::FolderId, links::create_link::CreateLinkError};
use garde::Validate;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use utoipa::ToSchema;

/// Request to create a document box
#[derive(Debug, Validate, Deserialize, ToSchema)]
pub struct CreateLink {
    /// Name for the link
    #[garde(length(min = 1, max = 255))]
    #[schema(min_length = 1, max_length = 255)]
    pub name: String,

    /// Link URL
    #[garde(length(min = 1))]
    #[schema(min_length = 1)]
    pub value: String,

    /// ID of the folder to store link in
    #[garde(skip)]
    #[schema(value_type = Uuid)]
    pub folder_id: FolderId,
}

/// Request to rename a file
#[derive(Debug, Validate, Deserialize, ToSchema)]
pub struct UpdateLinkRequest {
    /// Name for the link
    #[garde(inner(length(min = 1, max = 255)))]
    #[schema(min_length = 1, max_length = 255)]
    pub name: Option<String>,

    /// Value for the link
    #[garde(inner(length(min = 1)))]
    #[schema(min_length = 1)]
    pub value: Option<String>,

    /// New parent folder ID for the link
    #[garde(skip)]
    #[schema(value_type = Option<Uuid>)]
    pub folder_id: Option<FolderId>,

    /// Whether to pin the link
    #[garde(skip)]
    #[schema(value_type = Option<bool>)]
    pub pinned: Option<bool>,
}

/// Response metadata for a resolved link
#[derive(Debug, Serialize, ToSchema)]
pub struct LinkMetadataResponse {
    /// Title from the website metadata
    pub title: Option<String>,
    /// Alternative title from the website OGP metadata
    pub og_title: Option<String>,
    /// Description from the OGP metadata
    pub og_description: Option<String>,

    /// Whether the metadata resolved a favicon
    pub favicon: bool,
    /// Whether the metadata resolved a image
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
