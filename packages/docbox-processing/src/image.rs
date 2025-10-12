use super::{ProcessingError, ProcessingOutput, QueuedUpload};
use bytes::Bytes;
use docbox_database::models::generated_file::GeneratedFileType;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, ImageResult};
use std::io::Cursor;

/// Encodes the provided [DynamicImage] into a byte array
/// in the requested image `format`
pub fn create_img_bytes(image: &DynamicImage, format: ImageFormat) -> ImageResult<Vec<u8>> {
    let mut buffer = Cursor::new(Vec::new());
    image.write_to(&mut buffer, format)?;
    Ok(buffer.into_inner())
}

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
    let mut decoder = ImageReader::with_format(Cursor::new(&file_bytes), format)
        .into_decoder()
        .map_err(ProcessingError::DecodeImage)?;

    // Extract the image orientation
    let orientation = decoder
        .orientation()
        .map_err(ProcessingError::DecodeImage)?;

    let mut img = DynamicImage::from_decoder(decoder).map_err(ProcessingError::DecodeImage)?;

    // Apply image exif orientation
    img.apply_orientation(orientation);

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
) -> ImageResult<GeneratedPreviewImages> {
    tracing::debug!("rendering image preview variants");

    let thumbnail_jpeg = create_thumbnail(&image, format)?;
    let large_thumbnail_jpeg = create_thumbnail_large(&image, format)?;

    Ok(GeneratedPreviewImages {
        thumbnail_jpeg,
        large_thumbnail_jpeg,
    })
}

fn create_thumbnail(image: &DynamicImage, format: ImageFormat) -> ImageResult<Vec<u8>> {
    let thumbnail = image.thumbnail(64, 64);
    create_img_bytes(&thumbnail, format)
}

fn create_thumbnail_large(image: &DynamicImage, format: ImageFormat) -> ImageResult<Vec<u8>> {
    let (width, height) = match format {
        // .ico format has specific max size requirements
        ImageFormat::Ico => (256, 256),
        _ => (512, 512),
    };

    let cover_page_preview = image.resize(width, height, image::imageops::FilterType::Triangle);
    create_img_bytes(&cover_page_preview, format)
}
