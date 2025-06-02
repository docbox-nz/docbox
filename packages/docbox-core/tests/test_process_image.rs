use std::{io::Cursor, path::Path};

use bytes::Bytes;
use docbox_core::{
    image::{apply_exif_orientation, read_exif_orientation, Orientation},
    processing::image::process_image_async,
    utils::file::get_file_name_ext,
};
use image::{ImageFormat, ImageReader};

/// Tests that samples of supported image formats can be successfully processed
#[tokio::test]
async fn test_image_formats_supported() {
    let samples = [
        "sample.gif",
        "sample.ico",
        "sample.jpg",
        "sample.png",
        "sample.tif",
        "sample.webp",
    ];

    let samples_path = Path::new("tests/samples/image_processing");

    for sample in samples {
        let sample_file = samples_path.join(sample);
        let bytes = tokio::fs::read(sample_file).await.unwrap();
        let bytes = Bytes::from(bytes);
        let file_ext = get_file_name_ext(sample)
            .unwrap_or_else(|| panic!("Failed to get ext for'{}' ", sample));

        let image_format = ImageFormat::from_extension(file_ext)
            .unwrap_or_else(|| panic!("Failed to get mime type for'{}' ", sample));
        let _output = process_image_async(bytes, image_format).await.unwrap();
    }
}

/// Tests that samples with an associated exif orientation are detected
#[tokio::test]
async fn test_image_exif_data() {
    let samples = [
        ("sample_exif_horizontal.jpg", Orientation::Horizontal),
        (
            "sample_exif_mirror_horizontal_rotate_90.jpg",
            Orientation::MirrorHorizontalRotate90,
        ),
        (
            "sample_exif_mirror_horizontal_rotate_270.jpg",
            Orientation::MirrorHorizontalRotate270,
        ),
        (
            "sample_exif_mirror_horizontal.jpg",
            Orientation::MirrorHorizontal,
        ),
        (
            "sample_exif_mirror_vertical.jpg",
            Orientation::MirrorVertical,
        ),
        ("sample_exif_rotate_90.jpg", Orientation::Rotate90),
        ("sample_exif_rotate_180.jpg", Orientation::Rotate180),
        ("sample_exif_rotate_270.jpg", Orientation::Rotate270),
    ];

    let samples_path = Path::new("tests/samples/image_processing/exif");

    for (sample, sample_orientation) in samples {
        let sample_file = samples_path.join(sample);
        let bytes = tokio::fs::read(sample_file).await.unwrap();
        let bytes = Bytes::from(bytes);

        let orientation = read_exif_orientation(&bytes).unwrap();
        assert_eq!(orientation, sample_orientation);
    }
}

/// Tests that when applying orientation to samples the sample image size
/// matches the expected size for the new orientation
#[tokio::test]
async fn test_image_exif_data_apply() {
    let samples = [
        (
            "sample_exif_horizontal.jpg",
            Orientation::Horizontal,
            (32, 128),
        ),
        (
            "sample_exif_mirror_horizontal_rotate_90.jpg",
            Orientation::MirrorHorizontalRotate90,
            (128, 32),
        ),
        (
            "sample_exif_mirror_horizontal_rotate_270.jpg",
            Orientation::MirrorHorizontalRotate270,
            (128, 32),
        ),
        (
            "sample_exif_mirror_horizontal.jpg",
            Orientation::MirrorHorizontal,
            (32, 128),
        ),
        (
            "sample_exif_mirror_vertical.jpg",
            Orientation::MirrorVertical,
            (32, 128),
        ),
        (
            "sample_exif_rotate_90.jpg",
            Orientation::Rotate90,
            (128, 32),
        ),
        (
            "sample_exif_rotate_180.jpg",
            Orientation::Rotate180,
            (32, 128),
        ),
        (
            "sample_exif_rotate_270.jpg",
            Orientation::Rotate270,
            (128, 32),
        ),
    ];

    let samples_path = Path::new("tests/samples/image_processing/exif");

    for (sample, sample_orientation, expected_size) in samples {
        let sample_file = samples_path.join(sample);
        let bytes = tokio::fs::read(sample_file).await.unwrap();
        let bytes = Bytes::from(bytes);

        let orientation = read_exif_orientation(&bytes).unwrap();
        assert_eq!(orientation, sample_orientation);

        let img = ImageReader::with_format(Cursor::new(&bytes), ImageFormat::Jpeg)
            .decode()
            .unwrap();

        let img = apply_exif_orientation(img, orientation);

        let image_size = (img.width(), img.height());
        assert_eq!(image_size, expected_size);
    }
}
