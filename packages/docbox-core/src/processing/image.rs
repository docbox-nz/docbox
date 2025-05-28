use std::io::Cursor;

use super::{ProcessingError, ProcessingOutput};
use crate::{
    image::{apply_exif_orientation, create_img_bytes, read_exif_orientation},
    services::generated::QueuedUpload,
};
use bytes::Bytes;
use docbox_database::models::generated_file::GeneratedFileType;
use image::{DynamicImage, ImageFormat, ImageReader};

/// Image processing is CPU intensive, this async variant moves the image processing
/// to a separate thread where blocking is acceptable to prevent blocking other
/// asynchronous tasks
pub async fn process_image_async(
    file_bytes: Bytes,
    format: ImageFormat,
) -> Result<ProcessingOutput, ProcessingError> {
    tokio::task::spawn_blocking(move || process_image(file_bytes, format)).await?
}

/// Processes a compatible image file
///
/// Creates multiple small to medium sized thumbnail preview images for faster
/// previewing within the browser
fn process_image(
    file_bytes: Bytes,
    format: ImageFormat,
) -> Result<ProcessingOutput, ProcessingError> {
    let mut img = ImageReader::with_format(Cursor::new(&file_bytes), format)
        .decode()
        .map_err(ProcessingError::DecodeImage)?;

    // Process EXIF compatible formats to apply the right orientation
    if matches!(
        format,
        ImageFormat::Jpeg | ImageFormat::Tiff | ImageFormat::Png | ImageFormat::WebP
    ) {
        if let Some(orientation) = read_exif_orientation(&file_bytes) {
            img = apply_exif_orientation(img, orientation)
        }
    }

    tracing::debug!("generated image thumbnails");

    let generated =
        generate_image_preview(img, format).map_err(ProcessingError::GenerateThumbnail)?;

    let upload_queue = vec![
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
    ];

    Ok(ProcessingOutput {
        upload_queue,
        ..Default::default()
    })
}

/// Generated preview images for a file
struct GeneratedPreviewImages {
    /// Small 64x64 file thumbnail
    thumbnail_jpeg: Vec<u8>,
    /// Smaller 385x385 version of first page
    /// (Not actually 385x385 fits whatever the image aspect ratio inside those dimensions)
    large_thumbnail_jpeg: Vec<u8>,
}

fn generate_image_preview(
    image: DynamicImage,
    format: ImageFormat,
) -> anyhow::Result<GeneratedPreviewImages> {
    tracing::debug!("rendering image preview variants");

    let thumbnail_jpeg = {
        let thumbnail = image.thumbnail(64, 64);
        create_img_bytes(&thumbnail, ImageFormat::Jpeg)?
    };

    let large_thumbnail_jpeg = {
        let cover_page_preview = image.resize(512, 512, image::imageops::FilterType::Triangle);
        create_img_bytes(&cover_page_preview, format)?
    };

    Ok(GeneratedPreviewImages {
        thumbnail_jpeg,
        large_thumbnail_jpeg,
    })
}
