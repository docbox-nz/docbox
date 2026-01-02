use crate::error::HttpError;
use axum::http::StatusCode;
use axum_typed_multipart::{FieldData, TryFromMultipart};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use docbox_core::files::upload_file::UploadFileError;
use docbox_database::models::{
    file::{FileId, FileWithExtra},
    folder::FolderId,
    generated_file::GeneratedFile,
    presigned_upload_task::PresignedUploadTaskId,
    tasks::TaskId,
};
use docbox_processing::{ProcessingConfig, ProcessingError};
use garde::Validate;
use mime::Mime;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::{collections::HashMap, marker::PhantomData};
use thiserror::Error;
use utoipa::ToSchema;

/// Request to create a new presigned file upload
#[serde_as]
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreatePresignedRequest {
    /// Name of the file being uploaded
    #[garde(length(min = 1, max = 255))]
    #[schema(min_length = 1, max_length = 255)]
    pub name: String,

    /// ID of the folder to store the file in
    #[garde(skip)]
    #[schema(value_type = Uuid)]
    pub folder_id: FolderId,

    /// Size of the file being uploaded in bytes. Must match the size of the
    /// file being uploaded
    #[garde(range(min = 1))]
    #[schema(minimum = 1)]
    pub size: i32,

    /// Mime type of the file
    #[garde(skip)]
    #[serde_as(as = "Option<serde_with::DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    pub mime: Option<Mime>,

    /// Optional ID of the parent file if this file is associated as a child
    /// of another file. Mainly used to associating attachments to email files
    #[garde(skip)]
    #[schema(value_type = Option<Uuid>)]
    pub parent_id: Option<FileId>,

    /// Optional processing config
    #[garde(skip)]
    pub processing_config: Option<ProcessingConfig>,

    /// Whether to disable mime sniffing for the file. When false/not specified
    /// if a application/octet-stream mime type is provided the file name
    /// will be used to attempt to determine the real mime type
    #[garde(skip)]
    pub disable_mime_sniffing: Option<bool>,
}

/// Response describing how to upload the presigned file and the ID
/// for polling the progress
#[derive(Serialize, ToSchema)]
pub struct PresignedUploadResponse {
    /// ID of the file upload task to poll
    #[schema(value_type = Uuid)]
    pub task_id: PresignedUploadTaskId,
    /// HTTP method to use when uploading the file
    pub method: String,
    /// URL to upload the file to
    pub uri: String,
    /// Headers to include on the file upload request
    pub headers: HashMap<String, String>,
}

#[derive(Serialize, ToSchema)]
#[serde(tag = "status")]
#[allow(clippy::large_enum_variant)]
pub enum PresignedStatusResponse {
    /// Presigned upload is currently pending
    Pending,
    /// Presigned upload is completed
    Complete {
        /// The uploaded file
        file: FileWithExtra,
        /// The generated file
        generated: Vec<GeneratedFile>,
    },
    /// Presigned upload failed
    Failed {
        /// The error that occurred
        error: String,
    },
}

#[derive(TryFromMultipart, Validate, ToSchema)]
pub struct UploadFileRequest {
    /// Name of the file being uploaded
    #[garde(length(min = 1, max = 255))]
    #[schema(min_length = 1, max_length = 255)]
    pub name: String,

    /// ID of the folder to store the file in
    #[garde(skip)]
    #[schema(value_type = Uuid)]
    pub folder_id: FolderId,

    /// The actual file you are uploading, ensure the mime type for the file
    /// is set correctly
    #[garde(skip)]
    #[form_data(limit = "unlimited")]
    #[schema(format = Binary,value_type= Vec<u8>)]
    pub file: FieldData<Bytes>,

    /// Optional mime type override, when not present the mime type will
    /// be extracted from [UploadFileRequest::file]
    #[garde(skip)]
    pub mime: Option<String>,

    /// Whether to process the file asynchronously returning a task
    /// response instead of waiting for the upload
    #[garde(skip)]
    pub asynchronous: Option<bool>,

    /// Whether to disable mime sniffing for the file. When false/not specified
    /// if a application/octet-stream mime type is provided the file name
    /// will be used to attempt to determine the real mime type
    #[garde(skip)]
    pub disable_mime_sniffing: Option<bool>,

    /// Fixed file ID the file must use. Should only be used for
    /// migrating existing files and maintaining the same UUID.
    ///
    /// Should not be provided for general use
    #[garde(skip)]
    #[schema(value_type = Option<Uuid>)]
    pub fixed_id: Option<FileId>,

    /// Optional ID of the parent file if this file is associated as a child
    /// of another file. Mainly used to associating attachments to email files
    #[garde(skip)]
    #[schema(value_type = Option<Uuid>)]
    pub parent_id: Option<FileId>,

    /// Optional JSON encoded processing config
    #[garde(skip)]
    pub processing_config: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(untagged)]
pub enum FileUploadResponse {
    Sync(Box<UploadedFile>),
    Async(UploadTaskResponse),
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UploadedFile {
    /// The uploaded file itself
    pub file: FileWithExtra,
    /// Generated data alongside the file
    pub generated: Vec<GeneratedFile>,
    /// Additional files created and uploaded from processing the file
    #[schema(no_recursion)]
    pub additional_files: Vec<UploadedFile>,
}

/// Request to rename and or move a file
#[derive(Debug, Validate, Deserialize, ToSchema)]
pub struct UpdateFileRequest {
    /// Name for the folder
    #[garde(inner(length(min = 1, max = 255)))]
    #[schema(min_length = 1, max_length = 255)]
    pub name: Option<String>,

    /// New parent folder for the folder
    #[garde(skip)]
    #[schema(value_type = Option<Uuid>)]
    pub folder_id: Option<FolderId>,

    /// Whether to pin the file
    #[garde(skip)]
    #[schema(value_type = Option<bool>)]
    pub pinned: Option<bool>,
}

/// Response for requesting a document box
#[derive(Debug, Serialize, ToSchema)]
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
#[derive(Debug, Serialize, ToSchema)]
pub struct UploadTaskResponse {
    #[schema(value_type = Uuid)]
    pub task_id: TaskId,
    pub created_at: DateTime<Utc>,
}

/// Request to rename and or move a file
#[derive(Debug, Validate, Deserialize, ToSchema)]
pub struct GetPresignedRequest {
    /// Expiry time in seconds for the presigned URL
    #[garde(skip)]
    #[schema(default = 900)]
    pub expires_at: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct PresignedDownloadResponse {
    pub method: String,
    pub uri: String,
    pub headers: HashMap<String, String>,
    pub expires_at: DateTime<Utc>,
}

/// Type hint type for Utoipa to indicate a binary response type
#[derive(ToSchema)]
#[schema(value_type = String, format = Binary)]
pub struct BinaryResponse(PhantomData<Vec<u8>>);

#[derive(Debug, Error)]
pub enum HttpFileError {
    #[error("unknown file")]
    UnknownFile,

    #[error("unknown task")]
    UnknownTask,

    #[error("file size is larger than the maximum allowed size (requested: {0}, maximum: {1})")]
    FileTooLarge(i32, i32),

    #[error("fixed file id already in use")]
    FileIdInUse,

    #[error("request file mime content type is invalid")]
    InvalidMimeType,

    #[error("no matching generated file")]
    NoMatchingGenerated,

    #[allow(unused)]
    #[error("unsupported file type")]
    UnsupportedFileType,

    #[error(transparent)]
    UploadFileError(UploadFileError),
}

impl HttpError for HttpFileError {
    fn status(&self) -> axum::http::StatusCode {
        match self {
            HttpFileError::FileTooLarge(_, _) => StatusCode::BAD_REQUEST,
            HttpFileError::FileIdInUse => StatusCode::CONFLICT,
            HttpFileError::UnknownFile
            | HttpFileError::NoMatchingGenerated
            | HttpFileError::UnknownTask => StatusCode::NOT_FOUND,
            HttpFileError::UnsupportedFileType | HttpFileError::InvalidMimeType => {
                StatusCode::BAD_REQUEST
            }
            HttpFileError::UploadFileError(error) => match error {
                // Some processing errors can be assumed as the files fault
                UploadFileError::Processing(
                    ProcessingError::MalformedFile(_)
                    | ProcessingError::ReadPdfInfo(_)
                    | ProcessingError::ExtractFileText(_)
                    | ProcessingError::DecodeImage(_)
                    | ProcessingError::GenerateThumbnail(_)
                    | ProcessingError::Email(_),
                ) => StatusCode::UNPROCESSABLE_ENTITY,

                _ => StatusCode::INTERNAL_SERVER_ERROR,
            },
        }
    }
}
