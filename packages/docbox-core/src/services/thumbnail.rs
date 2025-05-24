use std::io::{BufWriter, Cursor, Write};

use anyhow::Context;
use image::{DynamicImage, ImageFormat};
use pdf_process::{render_single_page, OutputFormat, PdfInfo, RenderArgs};
use tracing::debug;

pub struct GeneratedPdfImages {
    /// Rendered full sized first page
    pub cover_page_jpeg: Vec<u8>,
    /// Small 64x64 file thumbnail
    pub thumbnail_jpeg: Vec<u8>,
    /// Smaller 385x385 version of first page
    /// (Not actually 385x385 fits whatever the image aspect ratio inside those dimensions)
    pub large_thumbnail_jpeg: Vec<u8>,
}

/// Renders the cover page for a PDF file
pub async fn render_pdf_cover(pdf_info: &PdfInfo, pdf: &[u8]) -> anyhow::Result<DynamicImage> {
    let args = RenderArgs::default();

    // Render the pdf cover page
    let page = render_single_page(pdf, pdf_info, OutputFormat::Jpeg, 1, &args)
        .await
        .context("failed to render pdf page")?;

    Ok(page)
}

fn create_cover_page(page: &DynamicImage, format: ImageFormat) -> anyhow::Result<Vec<u8>> {
    let encoded = create_img_bytes(page, format)?;
    Ok(encoded)
}

fn create_thumbnail(page: &DynamicImage, format: ImageFormat) -> anyhow::Result<Vec<u8>> {
    let thumbnail = page.thumbnail(64, 64);
    let encoded = create_img_bytes(&thumbnail, format)?;
    Ok(encoded)
}

pub fn create_cover_page_preview(
    page: &DynamicImage,
    format: ImageFormat,
) -> anyhow::Result<Vec<u8>> {
    let cover_page_preview = page.resize(512, 512, image::imageops::FilterType::Triangle);
    let encoded = create_img_bytes(&cover_page_preview, format)?;
    Ok(encoded)
}

pub async fn generate_pdf_images_async(
    pdf_info: &PdfInfo,
    pdf: &[u8],
) -> anyhow::Result<GeneratedPdfImages> {
    debug!("rendering pdf cover");
    let page = render_pdf_cover(pdf_info, pdf).await?;

    debug!("rendering pdf image variants");
    tokio::task::spawn_blocking(move || generate_pdf_images_variants(page))
        .await
        .context("failed to process image preview")
        .and_then(|value| value)
}

/// Generates the various versions of the PDF cover images
fn generate_pdf_images_variants(cover_page: DynamicImage) -> anyhow::Result<GeneratedPdfImages> {
    debug!("rendering pdf image variants");
    let cover_page_jpeg = create_cover_page(&cover_page, ImageFormat::Jpeg)?;
    let thumbnail_jpeg = create_thumbnail(&cover_page, ImageFormat::Jpeg)?;
    let large_thumbnail_jpeg = create_cover_page_preview(&cover_page, ImageFormat::Jpeg)?;

    Ok(GeneratedPdfImages {
        cover_page_jpeg,
        thumbnail_jpeg,
        large_thumbnail_jpeg,
    })
}

/// Generated preview images for a file
pub struct GeneratedPreviewImages {
    /// Small 64x64 file thumbnail
    pub thumbnail_jpeg: Vec<u8>,
    /// Smaller 385x385 version of first page
    /// (Not actually 385x385 fits whatever the image aspect ratio inside those dimensions)
    pub large_thumbnail_jpeg: Vec<u8>,
}

pub fn generate_image_preview(
    image: DynamicImage,
    format: ImageFormat,
) -> anyhow::Result<GeneratedPreviewImages> {
    debug!("rendering image preview variants");

    let thumbnail_jpeg = create_thumbnail(&image, format)?;
    let large_thumbnail_jpeg = create_cover_page_preview(&image, format)?;

    Ok(GeneratedPreviewImages {
        thumbnail_jpeg,
        large_thumbnail_jpeg,
    })
}

/// Encodes the provided image to bytes
pub fn create_img_bytes(
    image: &image::DynamicImage,
    format: ImageFormat,
) -> anyhow::Result<Vec<u8>> {
    let mut writer = BufWriter::new(Cursor::new(Vec::new()));

    // Convert the DynamicImage to JPEG format and save it
    image.write_to(&mut writer, format)?;
    writer.flush()?;
    let buffer = writer.into_inner()?;

    Ok(buffer.into_inner())
}
