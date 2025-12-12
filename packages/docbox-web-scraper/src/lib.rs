#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! # Docbox Web Scraper
//!
//! Web-scraping client for getting website metadata, favicon, ...etc and
//! maintaining an internal cache
//!
//! ## Environment Variables
//!
//! * `DOCBOX_WEB_SCRAPE_HTTP_PROXY` - Proxy server address to use for HTTP requests
//! * `DOCBOX_WEB_SCRAPE_HTTPS_PROXY` - Proxy server address to use for HTTPS requests
//! * `DOCBOX_WEB_SCRAPE_METADATA_CACHE_DURATION` - Time before cached metadata is considered expired
//! * `DOCBOX_WEB_SCRAPE_METADATA_CACHE_CAPACITY` - Maximum amount of metadata to cache at once
//! * `DOCBOX_WEB_SCRAPE_METADATA_CONNECT_TIMEOUT` - Timeout when connecting while scraping
//! * `DOCBOX_WEB_SCRAPE_METADATA_READ_TIMEOUT` - Timeout when reading responses from scraping
//! * `DOCBOX_WEB_SCRAPE_IMAGE_CACHE_DURATION` - Time before cached images are considered expired
//! * `DOCBOX_WEB_SCRAPE_IMAGE_CACHE_CAPACITY` - Maximum images to cache at once

use bytes::Bytes;
use document::{Favicon, determine_best_favicon, get_website_metadata};
use download_image::{ResolvedUri, download_image_href, resolve_full_url};
use mime::Mime;
use reqwest::Proxy;
use serde::{Deserialize, Serialize};
use std::{str::FromStr, time::Duration};
use thiserror::Error;
use url_validation::{TokioDomainResolver, is_allowed_url};

#[cfg(feature = "caching")]
mod cache;
mod data_uri;
mod document;
mod download_image;
mod url_validation;

#[cfg(feature = "caching")]
pub use cache::{
    CachingWebsiteMetaService, CachingWebsiteMetaServiceConfig,
    CachingWebsiteMetaServiceConfigError,
};
pub use reqwest::Url;

use crate::document::is_allowed_robots_txt;

/// Configuration for the website metadata service
#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct WebsiteMetaServiceConfig {
    /// HTTP proxy to use when making HTTP metadata requests
    pub http_proxy: Option<String>,
    /// HTTPS proxy to use when making HTTPS metadata requests
    pub https_proxy: Option<String>,
    /// Time to wait when attempting to fetch resource before timing out
    ///
    /// This option is ignored if you manually provide a [`reqwest::Client`]
    ///
    /// Default: 5s
    pub metadata_connect_timeout: Duration,
    /// Time to wait while downloading a resource before timing out (between each read of data)
    ///
    /// This option is ignored if you manually provide a [`reqwest::Client`]
    ///
    /// Default: 10s
    pub metadata_read_timeout: Duration,
}

/// Errors that could occur when loading the configuration
#[derive(Debug, Error)]
pub enum WebsiteMetaServiceConfigError {
    /// Provided connect timeout was an invalid number
    #[error("DOCBOX_WEB_SCRAPE_METADATA_CONNECT_TIMEOUT must be a number in seconds: {0}")]
    InvalidMetadataConnectTimeout(<u64 as FromStr>::Err),
    /// Provided read timeout was an invalid number
    #[error("DOCBOX_WEB_SCRAPE_METADATA_READ_TIMEOUT must be a number in seconds")]
    InvalidMetadataReadTimeout(<u64 as FromStr>::Err),
}

impl Default for WebsiteMetaServiceConfig {
    fn default() -> Self {
        Self {
            http_proxy: None,
            https_proxy: None,
            metadata_connect_timeout: Duration::from_secs(5),
            metadata_read_timeout: Duration::from_secs(10),
        }
    }
}

impl WebsiteMetaServiceConfig {
    /// Load a website meta service config from its environment variables
    pub fn from_env() -> Result<WebsiteMetaServiceConfig, WebsiteMetaServiceConfigError> {
        let mut config = WebsiteMetaServiceConfig {
            http_proxy: std::env::var("DOCBOX_WEB_SCRAPE_HTTP_PROXY").ok(),
            https_proxy: std::env::var("DOCBOX_WEB_SCRAPE_HTTPS_PROXY").ok(),
            ..Default::default()
        };

        if let Ok(metadata_connect_timeout) =
            std::env::var("DOCBOX_WEB_SCRAPE_METADATA_CONNECT_TIMEOUT")
        {
            let metadata_connect_timeout = metadata_connect_timeout
                .parse::<u64>()
                .map_err(WebsiteMetaServiceConfigError::InvalidMetadataConnectTimeout)?;

            config.metadata_connect_timeout = Duration::from_secs(metadata_connect_timeout);
        }

        if let Ok(metadata_read_timeout) = std::env::var("DOCBOX_WEB_SCRAPE_METADATA_READ_TIMEOUT")
        {
            let metadata_read_timeout = metadata_read_timeout
                .parse::<u64>()
                .map_err(WebsiteMetaServiceConfigError::InvalidMetadataReadTimeout)?;

            config.metadata_read_timeout = Duration::from_secs(metadata_read_timeout);
        }

        Ok(config)
    }
}

/// Service for looking up website metadata and storing a cached value
pub struct WebsiteMetaService {
    client: reqwest::Client,
}

/// Metadata resolved from a scraped website
#[derive(Clone, Serialize)]
pub struct ResolvedWebsiteMetadata {
    /// Title of the website from the `<title/>` element
    pub title: Option<String>,

    /// OGP title of the website
    pub og_title: Option<String>,

    /// OGP metadata description of the website
    pub og_description: Option<String>,

    /// Best determined image
    #[serde(skip)]
    pub og_image: Option<String>,

    /// Best determined favicon
    #[serde(skip)]
    pub best_favicon: Option<Favicon>,
}

/// Represents an image that has been resolved where the
/// contents are now know and the content type as well
#[derive(Debug, Clone)]
pub struct ResolvedImage {
    /// Content type of the image
    pub content_type: Mime,
    /// Byte contents of the resolved image
    pub bytes: Bytes,
}

impl WebsiteMetaService {
    /// Creates a new instance of the service, this initializes the HTTP
    /// client and creates the cache
    pub fn new() -> reqwest::Result<Self> {
        Self::from_config(Default::default())
    }

    /// Create a web scraper from the provided client
    pub fn from_client(client: reqwest::Client) -> Self {
        Self { client }
    }

    /// Create a web scraper from the provided config
    pub fn from_config(config: WebsiteMetaServiceConfig) -> reqwest::Result<Self> {
        let mut builder = reqwest::Client::builder();

        if let Some(http_proxy) = config.http_proxy.clone() {
            builder = builder.proxy(Proxy::http(http_proxy)?);
        }

        if let Some(https_proxy) = config.https_proxy.clone() {
            builder = builder.proxy(Proxy::https(https_proxy)?);
        }

        let client = builder
            .user_agent("DocboxLinkBot")
            .connect_timeout(config.metadata_connect_timeout)
            .read_timeout(config.metadata_read_timeout)
            .build()?;

        Ok(Self { client })
    }

    /// Resolves the metadata for the website at the provided URL
    pub async fn resolve_website(&self, url: &Url) -> Option<ResolvedWebsiteMetadata> {
        // Check if we are allowed to access the URL
        if !is_allowed_url::<TokioDomainResolver>(url).await {
            tracing::warn!("skipping resolve website metadata for disallowed url");
            return None;
        }

        // Check that the site allows scraping based on its robots.txt
        let is_allowed_scraping = is_allowed_robots_txt(&self.client, url)
            .await
            .unwrap_or(false);

        if !is_allowed_scraping {
            return None;
        }

        // Get the website metadata
        let res = match get_website_metadata(&self.client, url).await {
            Ok(value) => value,
            Err(cause) => {
                tracing::error!(?cause, "failed to get website metadata");
                return None;
            }
        };

        let best_favicon = determine_best_favicon(&res.favicons).cloned();

        Some(ResolvedWebsiteMetadata {
            title: res.title,
            og_title: res.og_title,
            og_description: res.og_description,
            og_image: res.og_image,
            best_favicon,
        })
    }

    /// Resolve the favicon image at the provided URL
    pub async fn resolve_website_favicon(&self, url: &Url) -> Option<ResolvedImage> {
        let website = self.resolve_website(url).await?;
        let favicon = match website.best_favicon {
            Some(best) => best.href,

            // No favicon from document? Fallback and try to use the default path
            None => {
                let mut url = url.clone();
                url.set_path("/favicon.ico");
                url.to_string()
            }
        };

        self.resolve_image(url, &favicon).await
    }

    /// Resolve the OGP metadata image from the provided URL
    pub async fn resolve_website_image(&self, url: &Url) -> Option<ResolvedImage> {
        let website = self.resolve_website(url).await?;
        let og_image = website.og_image?;

        self.resolve_image(url, &og_image).await
    }

    pub(crate) async fn resolve_image(&self, url: &Url, image: &str) -> Option<ResolvedImage> {
        let image_url = resolve_full_url(url, image).ok()?;

        // Check we are allowed to access the URL if its absolute
        if let ResolvedUri::Absolute(image_url) = &image_url
            && !is_allowed_url::<TokioDomainResolver>(image_url).await
        {
            tracing::warn!("skipping resolve image for disallowed url");
            return None;
        }

        let (bytes, content_type) = download_image_href(&self.client, image_url).await.ok()?;

        Some(ResolvedImage {
            content_type,
            bytes,
        })
    }
}
