//! Service that handles scraping websites.
//!
//! The service obtains the following:
//! - Favicon
//! - Page title
//! - OGP/Social Metadata (https://ogp.me/)
//!
//! It downloads the desired thumbnail and an OGP image if available
//! and stores it as [Bytes] in memory. Keeps an in-memory cache for
//! 48h of already visited websites

use anyhow::{anyhow, Context};
use bytes::Bytes;
use document::{determine_best_favicon, parse_website_metadata, WebsiteMetadata};
use download_image::download_image_href;
use http::{HeaderMap, HeaderValue};
use mime::Mime;
use moka::{future::Cache, policy::EvictionPolicy};
use reqwest::Url;
use serde::Serialize;
use std::time::Duration;
use tracing::error;
use url_validation::{is_allowed_url, TokioDomainResolver};

mod data_uri;
mod document;
mod download_image;
mod url_validation;

pub type OgpHttpClient = reqwest::Client;

/// Duration to maintain site metadata for (48h)
const METADATA_CACHE_DURATION: Duration = Duration::from_secs(60 * 60 * 48);

/// Time to wait when attempting to fetch resource before timing out
const METADATA_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// Time to wait while downloading a resource before timing out (between each read of data)
const METADATA_READ_TIMEOUT: Duration = Duration::from_secs(30);

/// Service for looking up website metadata and storing a cached value
#[derive(Clone)]
pub struct WebsiteMetaService {
    client: OgpHttpClient,
    cache: Cache<String, ResolvedWebsiteMetadata>,
}

#[derive(Clone, Serialize)]
pub struct ResolvedWebsiteMetadata {
    pub title: Option<String>,
    pub og_title: Option<String>,
    pub og_description: Option<String>,

    #[serde(skip)]
    pub og_image: Option<ResolvedImage>,
    #[serde(skip)]
    pub favicon: Option<ResolvedImage>,
}

/// Represents an image that has been resolved where the
/// contents are now know and the content type as well
#[derive(Debug, Clone)]
pub struct ResolvedImage {
    pub content_type: Mime,
    pub bytes: Bytes,
}

impl WebsiteMetaService {
    /// Creates a new instance of the service, this initializes the HTTP
    /// client and creates the cache
    pub fn new() -> anyhow::Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert("user-agent", HeaderValue::from_static("DocboxLinkBot"));

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .connect_timeout(METADATA_CONNECT_TIMEOUT)
            .read_timeout(METADATA_READ_TIMEOUT)
            .build()
            .context("failed to build http client")?;

        Ok(Self::from_client(client))
    }

    /// Create a web scraper from the provided client
    pub fn from_client(client: reqwest::Client) -> Self {
        let cache = Cache::builder()
            .time_to_idle(METADATA_CACHE_DURATION)
            .max_capacity(100)
            .eviction_policy(EvictionPolicy::tiny_lfu())
            .build();

        Self { client, cache }
    }

    /// Resolves the metadata for the website at the provided URL
    pub async fn resolve_website(&self, url: &str) -> anyhow::Result<ResolvedWebsiteMetadata> {
        // Cache hit
        if let Some(cached) = self.cache.get(url).await {
            return Ok(cached);
        }

        let url = reqwest::Url::parse(url).context("invalid resource url")?;

        // Assert we are allowed to access the URL
        if !is_allowed_url::<TokioDomainResolver>(&url).await {
            return Err(anyhow!("illegal url access"));
        }

        // Get the website metadata
        let res = get_website_metadata(&self.client, &url).await?;
        let best_favicon = determine_best_favicon(&res.favicons);

        // Download the favicon
        let favicon = match best_favicon {
            Some(best_favicon) => download_image_href(&self.client, &url, &best_favicon.href)
                .await
                .context("failed to load favicon")
                .map(|(bytes, content_type)| ResolvedImage {
                    bytes,
                    content_type,
                })
                .inspect_err(|cause| {
                    error!(%url, ?cause, "failed to resolve favicon");
                })
                .ok(),
            None => None,
        };

        // Download the OGP image
        let image = match res.og_image.as_ref() {
            Some(og_image) => download_image_href(&self.client, &url, og_image)
                .await
                .context("failed to load ogp image")
                .map(|(bytes, content_type)| ResolvedImage {
                    bytes,
                    content_type,
                })
                .inspect_err(|cause| {
                    error!(%url, ?cause, "failed to resolve valid ogp metadata image");
                })
                .ok(),
            None => None,
        };

        let resolved = ResolvedWebsiteMetadata {
            title: res.title,
            og_title: res.og_title,
            og_description: res.og_description,
            og_image: image,
            favicon,
        };

        // Cache the response
        self.cache.insert(url.to_string(), resolved.clone()).await;

        Ok(resolved)
    }
}

/// Connects to a website reading the HTML contents, extracts the metadata
/// required from the <head/> element
async fn get_website_metadata(
    client: &OgpHttpClient,
    url: &Url,
) -> anyhow::Result<WebsiteMetadata> {
    let mut url = url.clone();

    // Get the path from the URL
    let path = url.path();

    // Check if the path ends with a common HTML extension or if it is empty
    if !path.ends_with(".html") && !path.ends_with(".htm") && path.is_empty() {
        // Append /index.html if needed
        url.set_path("/index.html");
    }

    // Request page at URL
    let response = client
        .get(url)
        .send()
        .await
        .context("failed to request resource")?
        .error_for_status()
        .context("resource responded with error")?;

    // Read response text
    let text = response
        .text()
        .await
        .context("failed to read resource response")?;

    parse_website_metadata(&text)
}

#[cfg(test)]
mod test {
    use http::{HeaderMap, HeaderValue};
    use reqwest::Client;
    use url::Url;

    use super::{determine_best_favicon, download_image_href, get_website_metadata};

    #[tokio::test]
    #[ignore]
    async fn test_google_ogp() {
        let mut headers = HeaderMap::new();
        headers.insert("user-agent", HeaderValue::from_static("DocboxLinkBot"));

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .unwrap();

        let base_url: Url = "https://www.youtube.com/watch?v=suhEIUapSJQ"
            .parse()
            .unwrap();
        let res = get_website_metadata(&client, &base_url).await.unwrap();
        let best_favicon = determine_best_favicon(&res.favicons).unwrap();

        // let _bytes = download_remote_img(&client, base_url, &best_favicon.href)
        //     .await
        //     .unwrap();
        // let _bytes = download_remote_img(&client, base_url, &res.og_image.clone().unwrap())
        //     .await
        //     .unwrap();

        dbg!(&res, &best_favicon);
    }
    #[tokio::test]
    #[ignore]
    async fn test_base64_data_url() {
        let client = Client::default();

        let _bytes = download_image_href(&client, &"http://example.com".parse().unwrap(), "data:image/jpeg;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAB0lEQVR42mP8/wcAAwAB/8I+gQAAAABJRU5ErkJggg==").await.unwrap();

        dbg!(&_bytes);
    }
}
