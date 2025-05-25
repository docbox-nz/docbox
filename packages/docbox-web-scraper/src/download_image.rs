//! # Download Image
//!
//! Logic around downloading and resolving remote images

use crate::data_uri::{parse_data_uri, DataUriError};
use crate::url_validation::{is_allowed_url, TokioDomainResolver};
use bytes::Bytes;
use mime::Mime;
use reqwest::{header::CONTENT_TYPE, Url};
use thiserror::Error;
use tracing::{debug, error};

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

    /// Failed to create the full URL from its href
    #[error("invalid request url")]
    InvalidUrl,

    /// Requested URL was disallowed under the security requirements
    #[error("request url is not allowed")]
    DisallowedUrl,
}

/// Downloads an image file from a href relative to the `base_url`
pub async fn download_image_href(
    client: &reqwest::Client,
    base_url: &Url,
    href: &str,
) -> Result<(Bytes, Mime), DownloadImageError> {
    // Handle data URIs
    if href.starts_with("data:") {
        return parse_data_uri(href)
            .map_err(DownloadImageError::DataUri)
            .and_then(|(bytes, mime)| {
                // Ensure a valid mime type is present
                if mime.type_() != mime::IMAGE {
                    return Err(DownloadImageError::InvalidMimeType);
                }

                Ok((bytes, mime))
            });
    }

    let image_url = resolve_image_href(base_url, href).await?;
    debug!(%base_url, %href, "requesting remote image");
    download_image(client, image_url).await
}

/// Turns a image href into the full destination URL
/// handles absolute URLs and validates whether the
/// URL is allowed
async fn resolve_image_href(base_url: &Url, href: &str) -> Result<Url, DownloadImageError> {
    // Replace & encoding for query params
    let href = href.replace("&amp;", "&");

    // Resolve the full URL
    let url = if href.starts_with("http") {
        // If href is an absolute URL, use it directly
        Url::parse(&href)
    } else {
        // If href is a relative URL, resolve it against the base URL
        base_url.join(&href)
    }
    .map_err(|_| DownloadImageError::InvalidUrl)?;

    // Assert we are allowed to access the URL
    if !is_allowed_url::<TokioDomainResolver>(&url).await {
        return Err(DownloadImageError::DisallowedUrl);
    }

    Ok(url)
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
