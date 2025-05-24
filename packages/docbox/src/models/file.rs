use crate::{error::HttpError, MAX_FILE_SIZE};
use axum::http::StatusCode;
use axum_typed_multipart::{FieldData, TryFromMultipart};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use docbox_core::services::files::upload::ProcessingConfig;
use docbox_database::models::{
    file::{FileId, FileWithExtra},
    folder::FolderId,
    generated_file::GeneratedFile,
    presigned_upload_task::PresignedUploadTaskId,
    tasks::TaskId,
};
use garde::Validate;
use mime::Mime;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::collections::HashMap;
use thiserror::Error;

/// Request to create a new presigned file upload
#[serde_as]
#[derive(Deserialize, Validate)]
pub struct CreatePresignedRequest {
    /// Name of the file being uploaded
    #[garde(length(min = 1))]
    pub name: String,

    /// Folder to store the file in
    #[garde(skip)]
    pub folder_id: FolderId,

    /// Size of the file being uploaded
    #[garde(range(min = 1, max = MAX_FILE_SIZE as i32))]
    pub size: i32,

    /// Mime type of the file
    #[garde(skip)]
    #[serde_as(as = "serde_with::DisplayFromStr")]
    pub mime: Mime,

    /// Optional parent file ID
    #[garde(skip)]
    pub parent_id: Option<FileId>,

    /// Optional processing config
    #[garde(skip)]
    pub processing_config: Option<ProcessingConfig>,
}

#[derive(Serialize)]
pub struct PresignedUploadResponse {
    pub task_id: PresignedUploadTaskId,
    pub method: String,
    pub uri: String,
    pub headers: HashMap<String, String>,
}

#[derive(Serialize)]
#[serde(tag = "status")]
#[allow(clippy::large_enum_variant)]
pub enum PresignedStatusResponse {
    Pending,
    Complete {
        file: FileWithExtra,
        generated: Vec<GeneratedFile>,
    },
    Failed {
        error: String,
    },
}

#[derive(TryFromMultipart, Validate)]
pub struct UploadFileRequest {
    #[garde(length(min = 1))]
    pub name: String,

    /// Folder to store the file in
    #[garde(skip)]
    pub folder_id: FolderId,

    #[garde(skip)]
    #[form_data(limit = "unlimited")]
    pub file: FieldData<Bytes>,

    /// Whether to process the file asynchronously returning a task
    /// response instead of waiting for the upload
    #[garde(skip)]
    pub asynchronous: Option<bool>,

    /// Fixed file ID the file must use. Should only be used for
    /// migrating existing files and maintaining the same UUID.
    ///
    /// Should not be provided for general use
    #[garde(skip)]
    pub fixed_id: Option<FileId>,

    /// Optional ID of a parent file (i.e for email attachments)
    #[garde(skip)]
    pub parent_id: Option<FileId>,

    /// JSON encoded processing config
    #[garde(skip)]
    pub processing_config: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum FileUploadResponse {
    Sync(Box<UploadedFile>),
    Async(UploadTaskResponse),
}

#[derive(Debug, Serialize)]
pub struct UploadedFile {
    /// The uploaded file itself
    pub file: FileWithExtra,
    /// Generated data alongside the file
    pub generated: Vec<GeneratedFile>,
    /// Additional files created and uploaded from processing the file
    pub additional_files: Vec<UploadedFile>,
}

/// Request to rename and or move a file
#[derive(Debug, Validate, Deserialize)]
pub struct UpdateFileRequest {
    /// Name for the folder
    #[garde(inner(length(min = 1)))]
    pub name: Option<String>,

    /// New parent folder for the folder
    #[garde(skip)]
    pub folder_id: Option<FolderId>,
}

/// Response for requesting a document box
#[derive(Debug, Serialize)]
pub struct FileResponse {
    /// The file itself
    pub file: FileWithExtra,
    /// Files generated from the file (thumbnails, pdf, etc)
    pub generated: Vec<GeneratedFile>,
}

#[derive(Default, Debug, Deserialize)]
#[serde(default)]
pub struct RawFileQuery {
    pub download: bool,
}

/// Response from creating an upload
#[derive(Debug, Serialize)]
pub struct UploadTaskResponse {
    pub task_id: TaskId,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Error)]
pub enum HttpFileError {
    #[error("unknown file")]
    UnknownFile,

    #[error("unknown task")]
    UnknownTask,

    #[error("no matching generated file")]
    NoMatchingGenerated,

    #[allow(unused)]
    #[error("unsupported file type")]
    UnsupportedFileType,
}

impl HttpError for HttpFileError {
    fn log(&self) {}

    fn status(&self) -> axum::http::StatusCode {
        match self {
            HttpFileError::UnknownFile
            | HttpFileError::NoMatchingGenerated
            | HttpFileError::UnknownTask => StatusCode::NOT_FOUND,
            HttpFileError::UnsupportedFileType => StatusCode::BAD_REQUEST,
        }
    }
}
