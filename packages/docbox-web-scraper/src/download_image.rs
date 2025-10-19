//! # Download Image
//!
//! Logic around downloading and resolving remote images

use crate::data_uri::{DataUriError, parse_data_uri};
use bytes::Bytes;
use mime::Mime;
use reqwest::{Url, header::CONTENT_TYPE};
use thiserror::Error;
use tracing::{debug, error};

/// Error's that can occur when downloading an image
#[derive(Debug, Error)]
pub enum DownloadImageError {
    /// Error making the request
    #[error(transparent)]
    Request(reqwest::Error),

    /// Error as the response status
    #[error(transparent)]
    Response(reqwest::Error),

    /// Error when downloading the response
    #[error(transparent)]
    ResponseDownload(reqwest::Error),

    /// Mime type was missing or invalid
    #[error("content-type was missing or not an image mime type")]
    InvalidMimeType,

    /// Error related to a data uri
    #[error(transparent)]
    DataUri(DataUriError),
}

/// URI that has been resolved
pub enum ResolvedUri<'a> {
    /// Uri is a data URI
    Data(&'a str),

    /// Full absolute URL
    Absolute(Url),
}

/// Resolves a URL handling data URI's and ensuring the URLs are
/// absolute
pub fn resolve_full_url<'a>(
    base_url: &Url,
    href: &'a str,
) -> Result<ResolvedUri<'a>, url::ParseError> {
    if href.starts_with("data:") {
        return Ok(ResolvedUri::Data(href));
    }

    // Replace & encoding for query params
    let href = href.replace("&amp;", "&");

    // Resolve the full URL
    let url = if href.starts_with("http") {
        // If href is an absolute URL, use it directly
        Url::parse(&href)
    } else {
        // If href is a relative URL, resolve it against the base URL
        base_url.join(&href)
    }?;

    Ok(ResolvedUri::Absolute(url))
}

/// Downloads an image file from a href relative to the `base_url`
pub async fn download_image_href(
    client: &reqwest::Client,
    url: ResolvedUri<'_>,
) -> Result<(Bytes, Mime), DownloadImageError> {
    match url {
        // Handle data URIs
        ResolvedUri::Data(data_uri) => parse_data_uri(data_uri)
            .map_err(DownloadImageError::DataUri)
            .and_then(|(bytes, mime)| {
                // Ensure a valid mime type is present
                if mime.type_() != mime::IMAGE {
                    return Err(DownloadImageError::InvalidMimeType);
                }

                Ok((bytes, mime))
            }),

        ResolvedUri::Absolute(url) => {
            debug!(%url, "requesting remote image");
            download_image(client, url).await
        }
    }
}

/// Downloads an image from a `url` ensures the returned content-type
/// is an image before attempting to stream the download bytes. Will
/// error if the content-type is missing or not an image/* type
async fn download_image(
    client: &reqwest::Client,
    url: Url,
) -> Result<(Bytes, Mime), DownloadImageError> {
    // Request page at URL
    let response = client
        .get(url)
        .send()
        .await
        .map_err(DownloadImageError::Request)?
        .error_for_status()
        .map_err(DownloadImageError::Response)?;

    let headers = response.headers();
    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<Mime>().ok())
        .ok_or(DownloadImageError::InvalidMimeType)?;

    if content_type.type_() != mime::IMAGE {
        return Err(DownloadImageError::InvalidMimeType);
    }

    // Read response text
    let bytes = response
        .bytes()
        .await
        .map_err(DownloadImageError::ResponseDownload)?;

    Ok((bytes, content_type))
}
