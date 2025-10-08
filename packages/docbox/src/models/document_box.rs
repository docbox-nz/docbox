use std::{fmt::Display, str::FromStr};

use crate::error::HttpError;
use axum::http::StatusCode;
use docbox_database::models::{
    document_box::DocumentBox,
    folder::{FolderWithExtra, ResolvedFolderWithExtra},
};
use garde::Validate;
use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;
use utoipa::ToSchema;

/// Valid document box scope string, must be: A-Z, a-z, 0-9, ':', '-', '_', '.'
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, ToSchema, Serialize)]
#[serde(transparent)]
#[schema(example = "user:1:files", value_type = String)]
pub struct DocumentBoxScope(pub String);

impl Display for DocumentBoxScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

const ALLOWED_CHARS: [char; 4] = [':', '-', '_', '.'];

impl DocumentBoxScope {
    pub fn validate_scope(value: &str) -> bool {
        if value.trim().is_empty() {
            return false;
        }

        value
            .chars()
            .all(|char| char.is_ascii_alphanumeric() || ALLOWED_CHARS.contains(&char))
    }
}

impl<'de> Deserialize<'de> for DocumentBoxScope {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;

        if !DocumentBoxScope::validate_scope(&value) {
            return Err(serde::de::Error::custom(InvalidDocumentBoxScope));
        }

        Ok(Self(value))
    }
}

impl Validate for DocumentBoxScope {
    type Context = ();

    fn validate_into(
        &self,
        _ctx: &Self::Context,
        parent: &mut dyn FnMut() -> garde::Path,
        report: &mut garde::Report,
    ) {
        if !DocumentBoxScope::validate_scope(&self.0) {
            report.append(parent(), garde::Error::new("document box scope is invalid"))
        }
    }
}

#[derive(Debug, Error)]
#[error("invalid document box scope")]
pub struct InvalidDocumentBoxScope;

impl FromStr for DocumentBoxScope {
    type Err = InvalidDocumentBoxScope;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if !DocumentBoxScope::validate_scope(s) {
            return Err(InvalidDocumentBoxScope);
        }

        Ok(Self(s.to_string()))
    }
}

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
    /// Max allowed upload file size in bytes
    pub max_file_size: i32,
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
    /// Total size of the files contained within the document box
    pub file_size: i64,
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
