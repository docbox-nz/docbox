use crate::{
    files::generated::QueuedUpload,
    office::{OfficeConverter, PdfConvertError},
    processing::{pdf::process_pdf, ProcessingError, ProcessingOutput},
};
use bytes::Bytes;
use docbox_database::models::generated_file::GeneratedFileType;

#[derive(Clone)]
pub struct OfficeProcessingLayer {
    pub converter: OfficeConverter,
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
            return Err(ProcessingError::MalformedFile(
                "office file appears to be malformed failed conversion".to_string(),
            ))
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
