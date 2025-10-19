use base64::{Engine as _, engine::general_purpose::STANDARD};
use bytes::Bytes;
use mime::Mime;
use thiserror::Error;

/// Error's that can occur when parsing a data URI
#[derive(Debug, Error)]
pub enum DataUriError {
    /// Data URI was malformed in some way
    #[error("malformed data uris")]
    MalformedDataUri,

    /// Mime type for the data URL is invalid
    #[error("invalid data uri mime type")]
    InvalidMimeType(#[from] mime::FromStrError),

    /// Data URI parsing only supports base64 encoding at this stage
    /// any other format will give this error
    #[error("unsupported data uri format")]
    UnsupportedFormat,
}

/// Parses a base64 encoded data URI
pub fn parse_data_uri(uri: &str) -> Result<(Bytes, Mime), DataUriError> {
    // Strip the data prefix
    let data_url = uri
        .strip_prefix("data:")
        .ok_or(DataUriError::MalformedDataUri)?;

    let (raw_mime, data_url) = data_url
        .split_once(';')
        .ok_or(DataUriError::MalformedDataUri)?;

    let mime: Mime = raw_mime.parse()?;

    // Split the data URI into metadata and base64-encoded data
    let parts: Vec<&str> = data_url.split(',').collect();

    // Must have only two comma separated parts to the URI
    if parts.len() != 2 {
        return Err(DataUriError::MalformedDataUri);
    }

    let metadata = parts[0];
    let base64_data = parts[1];

    // Only base64 is supported
    if !metadata.contains("base64") {
        return Err(DataUriError::UnsupportedFormat);
    }

    // Decode the base64 uri data
    let data = STANDARD
        .decode(base64_data)
        .map(Bytes::from)
        .map_err(|_| DataUriError::MalformedDataUri)?;

    Ok((data, mime))
}

#[cfg(test)]
mod test {
    use crate::data_uri::DataUriError;

    use super::parse_data_uri;
    use mime::Mime;

    #[test]
    fn test_valid_data_uri() {
        let input = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAUA";
        let result = parse_data_uri(input);

        assert!(result.is_ok());
        let (data, mime) = result.unwrap();
        assert_eq!(mime, "image/png".parse::<Mime>().unwrap());
        assert!(!data.is_empty());
    }

    #[test]
    fn test_missing_data_prefix() {
        let input = "image/jpeg;base64,/9j/4AAQSkZJRgABAQAAAQABAAD";
        let result = parse_data_uri(input);
        assert!(matches!(result, Err(DataUriError::MalformedDataUri)));
    }

    #[test]
    fn test_missing_base64_format() {
        let input = "data:image/png;utf8,iVBORw0KGgoAAAANSUhEUgAAAAUA";
        let result = parse_data_uri(input);
        assert!(matches!(result, Err(DataUriError::UnsupportedFormat)));
    }

    #[test]
    fn test_malformed_uri_no_semicolon() {
        let input = "data:image/pngbase64,iVBORw0KGgoAAAANSUhEUgAAAAUA";
        let result = parse_data_uri(input);
        assert!(matches!(result, Err(DataUriError::MalformedDataUri)));
    }

    #[test]
    fn test_malformed_uri_no_comma() {
        let input = "data:image/png;base64iVBORw0KGgoAAAANSUhEUgAAAAUA";
        let result = parse_data_uri(input);
        assert!(matches!(result, Err(DataUriError::MalformedDataUri)));
    }

    #[test]
    fn test_invalid_mime_type() {
        let input = "data:invalidmime;base64,abcd";
        let result = parse_data_uri(input);
        assert!(matches!(result, Err(DataUriError::InvalidMimeType(_))));
    }

    #[test]
    fn test_invalid_base64_data() {
        let input = "data:image/png;base64,!!!not_base64!!!";
        let result = parse_data_uri(input);
        assert!(matches!(result, Err(DataUriError::MalformedDataUri)));
    }
}
