use super::thumbnail::generate_pdf_images_async;
use crate::{
    processing::{ProcessingError, ProcessingIndexMetadata, ProcessingOutput},
    search::models::DocumentPage,
    services::generated::QueuedUpload,
};
use docbox_database::models::generated_file::GeneratedFileType;
use futures::TryFutureExt;
use pdf_process::{
    pdf_info, text::PAGE_END_CHARACTER, text_all_pages_split, PdfInfoArgs, PdfInfoError,
    PdfTextArgs,
};

/// Processes a PDF compatible file producing index data and generated files such as
/// thumbnails and a converted pdf version
///
/// Extracts text from the PDF and creates multiple thumbnail preview images
/// of the first page at various sizes
pub async fn process_pdf(file_bytes: &[u8]) -> Result<ProcessingOutput, ProcessingError> {
    let pdf_info_args = PdfInfoArgs::default();

    // Load the pdf information
    let pdf_info = match pdf_info(file_bytes, &pdf_info_args).await {
        Ok(value) => value,
        // Skip processing encrypted pdf files
        Err(PdfInfoError::PdfEncrypted) => {
            return Ok(ProcessingOutput {
                encrypted: true,
                ..Default::default()
            });
        }
        // Handle invalid file
        Err(PdfInfoError::NotPdfFile) => return Err(ProcessingError::MalformedFile),

        // Handle other errors
        Err(cause) => {
            tracing::error!(?cause, "failed to get pdf file info");
            return Err(ProcessingError::ReadPdfInfo(cause));
        }
    };

    let page_count = pdf_info
        .pages()
        .ok_or(ProcessingError::MalformedFile)?
        .map_err(|_| ProcessingError::MalformedFile)?;

    // For processing the pdf file must have minimum 1 page
    if page_count < 1 {
        tracing::debug!("skipping processing on pdf with no pages");
        return Ok(ProcessingOutput::default());
    }

    tracing::debug!("generating file thumbnails & extracting text content");

    let text_args = PdfTextArgs::default();

    // Extract pdf text
    let pages_text_future = text_all_pages_split(file_bytes, &text_args)
        // Match outer result type with inner type
        .map_err(ProcessingError::ExtractFileText);

    // Generate pdf thumbnails
    let thumbnail_future = generate_pdf_images_async(&pdf_info, file_bytes)
        .map_err(ProcessingError::GenerateThumbnail);

    let (pages, generated) = tokio::try_join!(pages_text_future, thumbnail_future)?;

    // Create a combined text content using the PDF page end character
    let page_end = PAGE_END_CHARACTER.to_string();
    let combined_text_content = pages.join(&page_end).as_bytes().to_vec();

    let index_metadata = ProcessingIndexMetadata {
        pages: Some(
            pages
                .into_iter()
                .enumerate()
                .map(|(page, content)| DocumentPage {
                    page: page as u64,
                    content,
                })
                .collect(),
        ),
    };

    let upload_queue = vec![
        QueuedUpload::new(
            mime::IMAGE_JPEG,
            GeneratedFileType::CoverPage,
            generated.cover_page_jpeg.into(),
        ),
        QueuedUpload::new(
            mime::IMAGE_JPEG,
            GeneratedFileType::LargeThumbnail,
            generated.large_thumbnail_jpeg.into(),
        ),
        QueuedUpload::new(
            mime::IMAGE_JPEG,
            GeneratedFileType::SmallThumbnail,
            generated.thumbnail_jpeg.into(),
        ),
        QueuedUpload::new(
            mime::TEXT_PLAIN,
            GeneratedFileType::TextContent,
            combined_text_content.into(),
        ),
    ];

    Ok(ProcessingOutput {
        encrypted: false,
        additional_files: Default::default(),
        index_metadata: Some(index_metadata),
        upload_queue,
    })
}
