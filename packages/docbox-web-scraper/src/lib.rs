//! # Docbox Web Scraper
//!
//! Web-scraping client for getting website metadata, favicon, ...etc and
//! maintaining an internal cache

use bytes::Bytes;
use document::{determine_best_favicon, get_website_metadata, Favicon};
use download_image::{download_image_href, resolve_full_url, ResolvedUri};
use mime::Mime;
use moka::{future::Cache, policy::EvictionPolicy};
use serde::Serialize;
use std::time::Duration;
use url_validation::{is_allowed_url, TokioDomainResolver};

mod data_uri;
mod document;
mod download_image;
mod url_validation;

pub use reqwest::Url;

pub type OgpHttpClient = reqwest::Client;

/// Duration to maintain site metadata for (48h)
const METADATA_CACHE_DURATION: Duration = Duration::from_secs(60 * 60 * 48);
/// Maximum number of site metadata to maintain in the cache
const METADATA_CACHE_CAPACITY: u64 = 50;

/// Duration to maintain resolved images for (1h)
const IMAGE_CACHE_DURATION: Duration = Duration::from_secs(60 * 60);
/// Maximum number of images to maintain in the cache
const IMAGE_CACHE_CAPACITY: u64 = 50;

/// Time to wait when attempting to fetch resource before timing out
const METADATA_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// Time to wait while downloading a resource before timing out (between each read of data)
const METADATA_READ_TIMEOUT: Duration = Duration::from_secs(30);

/// Service for looking up website metadata and storing a cached value
pub struct WebsiteMetaService {
    client: OgpHttpClient,
    /// Cache for website metadata
    cache: Cache<String, Option<ResolvedWebsiteMetadata>>,
    /// Cache for resolved images will contain [None] for images that failed to load
    image_cache: Cache<(String, ImageCacheKey), Option<ResolvedImage>>,
}

/// Cache key for image cache value types
#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum ImageCacheKey {
    Favicon,
    Image,
}

#[derive(Clone, Serialize)]
pub struct ResolvedWebsiteMetadata {
    pub title: Option<String>,
    pub og_title: Option<String>,
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
    pub content_type: Mime,
    pub bytes: Bytes,
}

impl WebsiteMetaService {
    /// Creates a new instance of the service, this initializes the HTTP
    /// client and creates the cache
    pub fn new() -> reqwest::Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent("DocboxLinkBot")
            .connect_timeout(METADATA_CONNECT_TIMEOUT)
            .read_timeout(METADATA_READ_TIMEOUT)
            .build()?;

        Ok(Self::from_client(client))
    }

    /// Create a web scraper from the provided client
    pub fn from_client(client: reqwest::Client) -> Self {
        // Cache for metadata
        let cache = Cache::builder()
            .time_to_idle(METADATA_CACHE_DURATION)
            .max_capacity(METADATA_CACHE_CAPACITY)
            .eviction_policy(EvictionPolicy::tiny_lfu())
            .build();

        // Cache for loaded images
        let image_cache = Cache::builder()
            .time_to_idle(IMAGE_CACHE_DURATION)
            .max_capacity(IMAGE_CACHE_CAPACITY)
            .eviction_policy(EvictionPolicy::tiny_lfu())
            .build();

        Self {
            client,
            cache,
            image_cache,
        }
    }

    /// Resolves the metadata for the website at the provided URL
    pub async fn resolve_website(&self, url: &Url) -> Option<ResolvedWebsiteMetadata> {
        self.cache
            .get_with(url.to_string(), async move {
                // Check if we are allowed to access the URL
                if !is_allowed_url::<TokioDomainResolver>(url).await {
                    tracing::warn!("skipping resolve website metadata for disallowed url");
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
            })
            .await
    }

    pub async fn resolve_website_favicon(&self, url: &Url) -> Option<ResolvedImage> {
        let website = self.resolve_website(url).await?;
        let favicon = website.best_favicon?.href;

        self.resolve_image(url, ImageCacheKey::Favicon, favicon)
            .await
    }

    pub async fn resolve_website_image(&self, url: &Url) -> Option<ResolvedImage> {
        let website = self.resolve_website(url).await?;
        let og_image = website.og_image?;

        self.resolve_image(url, ImageCacheKey::Image, og_image)
            .await
    }

    async fn resolve_image(
        &self,
        url: &Url,
        cache_key: ImageCacheKey,
        image: String,
    ) -> Option<ResolvedImage> {
        self.image_cache
            .get_with((url.to_string(), cache_key), async move {
                let image_url = resolve_full_url(url, &image).ok()?;

                // Check we are allowed to access the URL if its absolute
                if let ResolvedUri::Absolute(image_url) = &image_url {
                    if !is_allowed_url::<TokioDomainResolver>(image_url).await {
                        tracing::warn!("skipping resolve image for disallowed url");
                        return None;
                    }
                }

                let (bytes, content_type) =
                    download_image_href(&self.client, image_url).await.ok()?;

                Some(ResolvedImage {
                    bytes,
                    content_type,
                })
            })
            .await
    }
}
