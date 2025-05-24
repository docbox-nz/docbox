use std::path::Path;

use mime::Mime;

use super::validation::MIME_EXT_MAP;

// Set of characters to allow in S3 file names a-zA-Z0-9
static ALLOWED_S3_CHARS: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

pub fn make_s3_safe(name: &str) -> String {
    name.chars()
        .filter_map(|c| {
            if c.is_whitespace() || c == '-' {
                // Replace whitespace and dashes with underscores
                Some('_')
            } else if ALLOWED_S3_CHARS.contains(c) {
                // Allowed characters can stay
                Some(c)
            } else {
                // Ignore anything else
                None
            }
        })
        // Don't take more than 50 chars worth of name
        .take(50)
        .collect()
}

/// Extracts the extension portion of a file name
pub fn get_file_name_ext(name: &str) -> Option<String> {
    let path = Path::new(name);
    let ext = path.extension()?;
    let ext = ext.to_str()?;
    Some(ext.to_string())
}

/// Finds the file extension to use for a file based on its mime type
pub fn get_mime_ext(mime: &Mime) -> Option<&'static str> {
    MIME_EXT_MAP.iter().find_map(|value| {
        if value.0.eq(mime.essence_str()) {
            Some(value.1)
        } else {
            None
        }
    })
}
