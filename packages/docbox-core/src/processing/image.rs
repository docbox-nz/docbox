use crate::services::{generated::QueuedUpload, thumbnail::generate_image_preview};
use bytes::Bytes;
use docbox_database::models::generated_file::GeneratedFileType;
use image::{DynamicImage, ImageFormat, ImageReader};
use std::io::Cursor;

use super::{ProcessingError, ProcessingOutput};

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
        img = apply_exif_orientation(img, &file_bytes)
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

/// Some images have the orientation supplied as EXIF metadata rather
/// than on the image itself, this function processes the image to apply that
/// EXIF metadata rotation directly to the image
///
/// `file_bytes` are required to read the EXIF metadata
fn apply_exif_orientation(img: DynamicImage, file_bytes: &[u8]) -> DynamicImage {
    // Parse EXIF data
    let mut reader = std::io::BufReader::new(Cursor::new(file_bytes));
    let exif_reader = exif::Reader::new();
    let exif = match exif_reader.read_from_container(&mut reader) {
        Ok(value) => value,

        // Failing to read the EXIF metadata is not considered a failure
        Err(cause) => {
            tracing::error!(?cause, "failed to read exif metadata");
            return img;
        }
    };

    let orientation = match exif
        .get_field(exif::Tag::Orientation, exif::In::PRIMARY)
        .and_then(|field| field.value.get_uint(0))
    {
        Some(value) => value,

        // Orientation not present, nothing to do
        None => return img,
    };

    // Valid orientations (From EXIF spec)
    // 1 = Horizontal (normal)
    // 2 = Mirror horizontal
    // 3 = Rotate 180
    // 4 = Mirror vertical
    // 5 = Mirror horizontal and rotate 270
    // 6 = Rotate 90
    // 7 = Mirror horizontal and rotate 90
    // 8 = Rotate 270
    match orientation {
        2 => img.fliph(),
        3 => img.rotate180(),
        4 => img.flipv(),
        5 => img.fliph().rotate270(),
        6 => img.rotate90(),
        7 => img.fliph().rotate90(),
        8 => img.rotate270(),

        // 1 and any invalid values should not alter the image
        _ => img,
    }
}
