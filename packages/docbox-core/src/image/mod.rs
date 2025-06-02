use image::{DynamicImage, ImageFormat, ImageResult};
use std::io::Cursor;

/// Encodes the provided [DynamicImage] into a byte array
/// in the requested image `format`
pub fn create_img_bytes(image: &DynamicImage, format: ImageFormat) -> ImageResult<Vec<u8>> {
    let mut buffer = Cursor::new(Vec::new());
    image.write_to(&mut buffer, format)?;
    Ok(buffer.into_inner())
}
