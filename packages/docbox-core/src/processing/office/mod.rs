use crate::{
    files::generated::QueuedUpload,
    processing::{
        ProcessingError, ProcessingOutput,
        office::convert_server::{
            OfficeConvertServerConfig, OfficeConvertServerError, is_known_pdf_convertable,
        },
        pdf::{is_pdf_file, process_pdf},
    },
};
use bytes::Bytes;
use convert_server::OfficeConverterServer;
use docbox_database::models::generated_file::GeneratedFileType;
use mime::Mime;
use office_convert_client::RequestError;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod convert_server;

const DISALLOW_MALFORMED_OFFICE: bool = true;

#[derive(Debug, Error)]
pub enum PdfConvertError {
    /// Failed to convert the file to a pdf
    #[error(transparent)]
    ConversionFailed(#[from] RequestError),

    #[error("office document is malformed")]
    MalformedDocument,

    #[error("office document is password protected")]
    EncryptedDocument,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OfficeConverterConfig {
    ConverterServer(OfficeConvertServerConfig),
}

impl OfficeConverterConfig {
    pub fn from_env() -> OfficeConverterConfig {
        let config = OfficeConvertServerConfig::from_env();
        OfficeConverterConfig::ConverterServer(config)
    }
}

#[derive(Clone)]
pub enum OfficeConverter {
    ConverterServer(OfficeConverterServer),
}

#[derive(Debug, Error)]
pub enum OfficeConverterError {
    #[error(transparent)]
    ConverterServer(#[from] OfficeConvertServerError),
}

#[derive(Clone)]
pub struct OfficeProcessingLayer {
    pub converter: OfficeConverter,
}

impl OfficeConverter {
    pub fn from_config(
        config: OfficeConverterConfig,
    ) -> Result<OfficeConverter, OfficeConverterError> {
        match config {
            OfficeConverterConfig::ConverterServer(config) => {
                let converter_server = OfficeConverterServer::from_config(config)?;
                Ok(OfficeConverter::ConverterServer(converter_server))
            }
        }
    }

    pub async fn convert_to_pdf(&self, bytes: Bytes) -> Result<Bytes, PdfConvertError> {
        match self {
            OfficeConverter::ConverterServer(inner) => inner.convert_to_pdf(bytes).await,
        }
    }

    pub fn is_convertable(&self, mime: &Mime) -> bool {
        match self {
            OfficeConverter::ConverterServer(inner) => inner.is_convertable(mime),
        }
    }
}

/// Trait for converting some file input bytes into some output bytes
/// for a converted PDF file
pub(crate) trait ConvertToPdf {
    async fn convert_to_pdf(&self, bytes: Bytes) -> Result<Bytes, PdfConvertError>;

    fn is_convertable(&self, mime: &Mime) -> bool;
}

/// Checks if the provided mime type either is a PDF
/// or can be converted to a PDF
pub fn is_pdf_compatible(mime: &Mime) -> bool {
    is_pdf_file(mime) || is_known_pdf_convertable(mime)
}

/// Processes a PDF compatible office/other supported file format. Converts to
/// PDF then processes as a PDF with [process_pdf]
pub async fn process_office(
    layer: &OfficeProcessingLayer,
    file_bytes: Bytes,
) -> Result<ProcessingOutput, ProcessingError> {
    // Convert file to a pdf
    let file_bytes = match layer.converter.convert_to_pdf(file_bytes).await {
        Ok(value) => value,

        // Encrypted document
        Err(PdfConvertError::EncryptedDocument) => {
            return Ok(ProcessingOutput {
                encrypted: true,
                ..Default::default()
            });
        }

        // Malformed document
        Err(PdfConvertError::MalformedDocument) => {
            if DISALLOW_MALFORMED_OFFICE {
                return Err(ProcessingError::MalformedFile(
                    "office file appears to be malformed failed conversion".to_string(),
                ));
            }

            return Ok(ProcessingOutput::default());
        }

        // Other error
        Err(cause) => {
            tracing::error!(?cause, "failed to convert document to pdf");
            return Err(ProcessingError::ConvertFile(cause));
        }
    };

    let mut output = process_pdf(&file_bytes).await?;

    // Store the converted pdf file
    output.upload_queue.push(QueuedUpload::new(
        mime::APPLICATION_PDF,
        GeneratedFileType::Pdf,
        file_bytes,
    ));

    Ok(output)
}
