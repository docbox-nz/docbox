use image::{DynamicImage, ImageFormat, ImageResult};
use std::io::Cursor;

/// Encodes the provided [DynamicImage] into a byte array
/// in the requested image `format`
pub fn create_img_bytes(image: &DynamicImage, format: ImageFormat) -> ImageResult<Vec<u8>> {
    let mut buffer = Cursor::new(Vec::new());
    image.write_to(&mut buffer, format)?;
    Ok(buffer.into_inner())
}

/// Read the EXIF orientation
pub fn read_exif_orientation(file: &[u8]) -> Option<Orientation> {
    // Parse EXIF data
    let mut reader = Cursor::new(file);
    let exif_reader = exif::Reader::new();
    let exif = exif_reader
        .read_from_container(&mut reader)
        // Failing to read the EXIF metadata is not considered a failure
        // often occurs just because the file type is not one thats supported
        .inspect_err(|cause| {
            tracing::warn!(?cause, "failed to read exif metadata");
        })
        .ok()?;

    let orientation = exif
        .get_field(exif::Tag::Orientation, exif::In::PRIMARY)
        .and_then(|field| field.value.get_uint(0))?;

    Some(Orientation::from(orientation))
}
// Valid orientations (From EXIF spec)
pub enum Orientation {
    // 1 = Horizontal (normal)
    Horizontal,
    // 2 = Mirror horizontal
    MirrorHorizontal,
    // 3 = Rotate 180
    Rotate180,
    // 4 = Mirror vertical
    MirrorVertical,
    // 5 = Mirror horizontal and rotate 270
    MirrorHorizontalRotate270,
    // 6 = Rotate 90
    Rotate90,
    // 7 = Mirror horizontal and rotate 90
    MirrorHorizontalRotate90,
    // 8 = Rotate 270
    Rotate270,
    Other(u32),
}

impl From<u32> for Orientation {
    fn from(value: u32) -> Self {
        match value {
            1 => Orientation::Horizontal,
            2 => Orientation::MirrorHorizontal,
            3 => Orientation::Rotate180,
            4 => Orientation::MirrorVertical,
            5 => Orientation::MirrorHorizontalRotate270,
            6 => Orientation::Rotate90,
            7 => Orientation::MirrorHorizontalRotate90,
            8 => Orientation::Rotate270,
            other => Orientation::Other(other),
        }
    }
}

/// Some images have the orientation supplied as EXIF metadata rather
/// than on the image itself, this function processes the image to apply that
/// EXIF metadata rotation directly to the image
///
/// `file_bytes` are required to read the EXIF metadata
pub fn apply_exif_orientation(img: DynamicImage, orientation: Orientation) -> DynamicImage {
    match orientation {
        Orientation::Horizontal | Orientation::Other(_) => img,
        Orientation::MirrorHorizontal => img.fliph(),
        Orientation::Rotate180 => img.rotate180(),
        Orientation::MirrorVertical => img.flipv(),
        Orientation::MirrorHorizontalRotate270 => img.fliph().rotate270(),
        Orientation::Rotate90 => img.rotate90(),
        Orientation::MirrorHorizontalRotate90 => img.fliph().rotate90(),
        Orientation::Rotate270 => img.rotate270(),
    }
}
