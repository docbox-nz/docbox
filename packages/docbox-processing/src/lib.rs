#![forbid(unsafe_code)]

use std::{num::ParseIntError, str::FromStr, time::Duration};

use crate::{
    email::{EmailProcessingError, is_mail_mime, process_email},
    image::process_image_async,
    office::{PdfConvertError, process_office},
    pdf::{GeneratePdfImagesError, process_pdf},
};
use ::image::{ImageError, ImageFormat};
use bytes::Bytes;
use docbox_database::models::{file::FileId, generated_file::GeneratedFileType};
use docbox_search::models::DocumentPage;
use mime::Mime;
use office::OfficeProcessingLayer;
use pdf::is_pdf_file;
use pdf_process::{PdfInfoError, PdfTextError};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::task::JoinError;
use utoipa::ToSchema;

pub mod email;
pub mod html_to_text;
pub mod image;
pub mod office;
pub mod pdf;

#[derive(Debug, Error)]
pub enum ProcessingError {
    /// Uploaded file is malformed or unprocessable
    #[error("file is invalid or malformed: {0}")]
    MalformedFile(String),

    /// Failed to convert file to pdf
    #[error("failed to convert file")]
    ConvertFile(#[from] PdfConvertError),

    /// Failed to read info about pdf file
    #[error("failed to read pdf info")]
    ReadPdfInfo(PdfInfoError),

    /// Failed to extract text from pdf file
    #[error("failed to extract pdf file text: {0}")]
    ExtractFileText(PdfTextError),

    /// Failed to decode an image to generate thumbnails
    #[error("failed to decode image file: {0}")]
    DecodeImage(ImageError),

    /// Failed to generate thumbnail from pdf file
    #[error("failed to generate file thumbnail: {0}")]
    GenerateThumbnail(ImageError),

    /// Failed to generate thumbnail from pdf file
    #[error("failed to generate pdf file thumbnail: {0}")]
    GeneratePdfThumbnail(GeneratePdfImagesError),

    /// Failed to process an email file
    #[error("failed to process email file: {0}")]
    Email(#[from] EmailProcessingError),

    /// Failed to join the image processing thread output
    #[error("error waiting for image processing")]
    Threading(#[from] JoinError),
}

#[derive(Debug, Default, Clone, Deserialize, Serialize, ToSchema)]
#[serde(default)]
pub struct ProcessingConfig {
    /// Email specific processing configuration
    pub email: Option<EmailProcessingConfig>,

    /// Maximum number of times to unpack a file. When unpacking
    /// things like email attachments, these are recursively this
    /// limits the number of nested unpacking that can occur.
    ///
    /// Default: 1 (Unpack Only the immediate children)
    pub max_unpack_iterations: Option<usize>,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize, ToSchema)]
#[serde(default)]
pub struct EmailProcessingConfig {
    /// Whether to skip extracting attachments when processing an email
    pub skip_attachments: Option<bool>,
}

#[derive(Debug)]
pub struct QueuedUpload {
    pub mime: Mime,
    pub ty: GeneratedFileType,
    pub bytes: Bytes,
}

impl QueuedUpload {
    pub fn new(mime: Mime, ty: GeneratedFileType, bytes: Bytes) -> Self {
        Self { mime, ty, bytes }
    }
}

/// Represents a file that should be created and processed as the
/// output of processing a file
#[derive(Debug)]
pub struct AdditionalProcessingFile {
    /// Specify a fixed ID to use for the processed file output
    pub fixed_id: Option<FileId>,
    /// Name of the file
    pub name: String,
    /// Mime type of the file to process
    pub mime: Mime,
    /// Bytes of the file
    pub bytes: Bytes,
}

#[derive(Debug, Default)]
pub struct ProcessingOutput {
    /// Files that are waiting to be uploaded to S3
    pub upload_queue: Vec<QueuedUpload>,

    /// Collection of additional files that also need to be
    /// processed
    pub additional_files: Vec<AdditionalProcessingFile>,

    /// Data that should be persisted to the search index
    pub index_metadata: Option<ProcessingIndexMetadata>,

    /// Whether the file has be detected as encrypted
    pub encrypted: bool,
}

#[derive(Debug, Default)]
pub struct ProcessingIndexMetadata {
    /// Optional page text metadata extracted from the file
    pub pages: Option<Vec<DocumentPage>>,
}

#[derive(Clone)]
pub struct ProcessingLayer {
    pub office: OfficeProcessingLayer,
    pub config: ProcessingLayerConfig,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct ProcessingLayerConfig {
    /// Maximum number of times to unpack a file. When unpacking
    /// things like email attachments, these are recursively this
    /// limits the number of nested unpacking that can occur.
    ///
    /// This is the maximum allowed iterations on the server level,
    /// requests can specify a custom amount but this amount is
    /// capped to this value
    ///
    /// Default: 1 (Unpack Only the immediate children)
    pub max_unpack_iterations: Option<usize>,

    /// Maximum duration to allow the file processing to run for
    /// the processing will be terminated if it takes longer than
    /// this duration to run.
    ///
    /// Default: 300s
    pub process_timeout: Option<Duration>,
}

pub const DEFAULT_PROCESS_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Error)]
pub enum ProcessingLayerConfigError {
    /// Value provided for max unpack iterations was invalid
    #[error("invalid DOCBOX_MAX_FILE_UNPACK_ITERATIONS value must be a number")]
    InvalidMaxIterations(ParseIntError),
    /// Invalid process timeout seconds
    #[error("DOCBOX_FILE_PROCESSING_TIMEOUT must be a number in seconds")]
    InvalidProcessTimeout(<u64 as FromStr>::Err),
}

impl ProcessingLayerConfig {
    pub fn from_env() -> Result<ProcessingLayerConfig, ProcessingLayerConfigError> {
        let max_unpack_iterations = std::env::var("DOCBOX_MAX_FILE_UNPACK_ITERATIONS")
            .ok()
            .map(|value| {
                value
                    .parse::<usize>()
                    .map_err(ProcessingLayerConfigError::InvalidMaxIterations)
            })
            .transpose()?;

        let process_timeout = std::env::var("DOCBOX_FILE_PROCESSING_TIMEOUT")
            .ok()
            .map(|process_timeout| {
                process_timeout
                    .parse::<u64>()
                    .map_err(ProcessingLayerConfigError::InvalidProcessTimeout)
                    .map(Duration::from_secs)
            })
            .transpose()?;

        Ok(ProcessingLayerConfig {
            max_unpack_iterations,
            process_timeout,
        })
    }
}

/// Processes a file returning the generated processing output
///
/// # Arguments
/// * `config` - Optional config for processing
/// * `converter` - Converter for office files
/// * `file_bytes` - Actual byte contents of the file
/// * `mime` - Mime type of the file being processed
pub async fn process_file(
    config: &Option<ProcessingConfig>,
    layer: &ProcessingLayer,
    bytes: Bytes,
    mime: &Mime,
) -> Result<Option<ProcessingOutput>, ProcessingError> {
    // File is a PDF
    if is_pdf_file(mime) {
        tracing::debug!("processing pdf file");

        let output = process_pdf(&bytes).await?;
        Ok(Some(output))
    }
    // File can be converted to a PDF then processed
    else if layer.office.converter.is_convertable(mime) {
        tracing::debug!("processing office compatible file");

        let output = process_office(&layer.office, bytes).await?;
        Ok(Some(output))
    }
    // File is an email
    else if is_mail_mime(mime) {
        tracing::debug!("processing email file");

        let output = process_email(config, &bytes)?;
        Ok(Some(output))
    }
    // Process image files if the file type is known and can be processed
    else if let Some(image_format) = ImageFormat::from_mime_type(mime) {
        tracing::debug!("processing image file");

        let output = process_image_async(bytes, image_format).await?;
        Ok(Some(output))
    }
    // No processing for this file type
    else {
        Ok(None)
    }
}
