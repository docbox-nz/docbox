use crate::{
    files::generated::QueuedUpload,
    image::create_img_bytes,
    processing::{ProcessingError, ProcessingIndexMetadata, ProcessingOutput},
};
use anyhow::Context;
use docbox_database::models::generated_file::GeneratedFileType;
use docbox_search::models::DocumentPage;
use futures::TryFutureExt;
use image::{DynamicImage, ImageFormat};
use mime::Mime;
use pdf_process::{
    OutputFormat, PdfInfo, PdfInfoArgs, PdfInfoError, PdfTextArgs, RenderArgs, pdf_info,
    render_single_page, text_all_pages_split,
};

pub use pdf_process::text::PAGE_END_CHARACTER;

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
        Err(PdfInfoError::NotPdfFile) => {
            return Err(ProcessingError::MalformedFile(
                "file was not a pdf file".to_string(),
            ));
        }

        // Handle other errors
        Err(cause) => {
            tracing::error!(?cause, "failed to get pdf file info");
            return Err(ProcessingError::ReadPdfInfo(cause));
        }
    };

    let page_count = pdf_info
        .pages()
        .ok_or_else(|| {
            ProcessingError::MalformedFile("failed to determine page count".to_string())
        })?
        .map_err(|err| {
            ProcessingError::MalformedFile(format!(
                "failed to convert pages number to integer: {err}"
            ))
        })?;

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

#[inline]
pub fn is_pdf_file(mime: &Mime) -> bool {
    if mime.eq(&mime::APPLICATION_PDF) {
        return true;
    }

    // Some outdated clients use application/x-pdf for pdfs
    if mime.type_() == mime::APPLICATION && mime.subtype().as_str() == "x-pdf" {
        return true;
    }

    false
}

/// Renders the cover page for a PDF file
async fn render_pdf_cover(pdf_info: &PdfInfo, pdf: &[u8]) -> anyhow::Result<DynamicImage> {
    let args = RenderArgs::default();

    // Render the pdf cover page
    let page = render_single_page(pdf, pdf_info, OutputFormat::Jpeg, 1, &args)
        .await
        .context("failed to render pdf page")?;

    Ok(page)
}

pub struct GeneratedPdfImages {
    /// Rendered full sized first page
    pub cover_page_jpeg: Vec<u8>,
    /// Small 64x64 file thumbnail
    pub thumbnail_jpeg: Vec<u8>,
    /// Smaller 385x385 version of first page
    /// (Not actually 385x385 fits whatever the image aspect ratio inside those dimensions)
    pub large_thumbnail_jpeg: Vec<u8>,
}

async fn generate_pdf_images_async(
    pdf_info: &PdfInfo,
    pdf: &[u8],
) -> anyhow::Result<GeneratedPdfImages> {
    tracing::debug!("rendering pdf cover");
    let page = render_pdf_cover(pdf_info, pdf).await?;

    tracing::debug!("rendering pdf image variants");
    tokio::task::spawn_blocking(move || generate_pdf_images_variants(page))
        .await
        .context("failed to process image preview")
        .and_then(|value| value)
}

/// Generates the various versions of the PDF cover images
fn generate_pdf_images_variants(cover_page: DynamicImage) -> anyhow::Result<GeneratedPdfImages> {
    tracing::debug!("rendering pdf image variants");
    let cover_page_jpeg = create_img_bytes(&cover_page, ImageFormat::Jpeg)?;

    let thumbnail_jpeg = {
        let thumbnail = cover_page.thumbnail(64, 64);
        create_img_bytes(&thumbnail, ImageFormat::Jpeg)?
    };

    let large_thumbnail_jpeg = {
        let cover_page_preview = cover_page.resize(512, 512, image::imageops::FilterType::Triangle);
        create_img_bytes(&cover_page_preview, ImageFormat::Jpeg)?
    };

    Ok(GeneratedPdfImages {
        cover_page_jpeg,
        thumbnail_jpeg,
        large_thumbnail_jpeg,
    })
}
