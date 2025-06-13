//! # Docbox Web Scraper
//!
//! Web-scraping client for getting website metadata, favicon, ...etc and
//! maintaining an internal cache

use anyhow::Context;
use bytes::Bytes;
use document::{Favicon, determine_best_favicon, get_website_metadata};
use download_image::{ResolvedUri, download_image_href, resolve_full_url};
use mime::Mime;
use moka::{future::Cache, policy::EvictionPolicy};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use url_validation::{TokioDomainResolver, is_allowed_url};

mod data_uri;
mod document;
mod download_image;
mod url_validation;

pub use reqwest::Url;

pub type OgpHttpClient = reqwest::Client;

#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct WebsiteMetaServiceConfig {
    /// Duration to maintain site metadata for (48h)
    pub metadata_cache_duration: Duration,
    /// Maximum number of site metadata to maintain in the cache
    pub metadata_cache_capacity: u64,

    /// Duration to maintain resolved images for (15min)
    pub image_cache_duration: Duration,
    /// Maximum number of images to maintain in the cache
    pub image_cache_capacity: u64,

    /// Time to wait when attempting to fetch resource before timing out
    ///
    /// This option is ignored if you manually provide a [reqwest::Client]
    pub metadata_connect_timeout: Duration,
    /// Time to wait while downloading a resource before timing out (between each read of data)
    ///
    /// This option is ignored if you manually provide a [reqwest::Client]
    pub metadata_read_timeout: Duration,
}

impl WebsiteMetaServiceConfig {
    /// Load a website meta service config from its environment variables
    pub fn from_env() -> anyhow::Result<WebsiteMetaServiceConfig> {
        let mut config = WebsiteMetaServiceConfig::default();

        if let Ok(metadata_cache_duration) =
            std::env::var("DOCBOX_WEB_SCRAPE_METADATA_CACHE_DURATION")
        {
            let metadata_cache_duration = metadata_cache_duration
                .parse::<u64>()
                .context("DOCBOX_WEB_SCRAPE_METADATA_CACHE_DURATION must be a number in seconds")?;

            config.metadata_cache_duration = Duration::from_secs(metadata_cache_duration);
        }

        if let Ok(metadata_cache_capacity) =
            std::env::var("DOCBOX_WEB_SCRAPE_METADATA_CACHE_CAPACITY")
        {
            let metadata_cache_capacity = metadata_cache_capacity
                .parse::<u64>()
                .context("DOCBOX_WEB_SCRAPE_METADATA_CACHE_CAPACITY must be a number")?;

            config.metadata_cache_capacity = metadata_cache_capacity;
        }

        if let Ok(metadata_connect_timeout) =
            std::env::var("DOCBOX_WEB_SCRAPE_METADATA_CONNECT_TIMEOUT")
        {
            let metadata_connect_timeout = metadata_connect_timeout.parse::<u64>().context(
                "DOCBOX_WEB_SCRAPE_METADATA_CONNECT_TIMEOUT must be a number in seconds",
            )?;

            config.metadata_connect_timeout = Duration::from_secs(metadata_connect_timeout);
        }

        if let Ok(metadata_read_timeout) = std::env::var("DOCBOX_WEB_SCRAPE_METADATA_READ_TIMEOUT")
        {
            let metadata_read_timeout = metadata_read_timeout
                .parse::<u64>()
                .context("DOCBOX_WEB_SCRAPE_METADATA_READ_TIMEOUT must be a number in seconds")?;

            config.metadata_read_timeout = Duration::from_secs(metadata_read_timeout);
        }

        if let Ok(image_cache_duration) = std::env::var("DOCBOX_WEB_SCRAPE_IMAGE_CACHE_DURATION") {
            let image_cache_duration = image_cache_duration
                .parse::<u64>()
                .context("DOCBOX_WEB_SCRAPE_IMAGE_CACHE_DURATION must be a number in seconds")?;

            config.image_cache_duration = Duration::from_secs(image_cache_duration);
        }

        if let Ok(image_cache_capacity) = std::env::var("DOCBOX_WEB_SCRAPE_IMAGE_CACHE_CAPACITY") {
            let image_cache_capacity = image_cache_capacity
                .parse::<u64>()
                .context("DOCBOX_WEB_SCRAPE_METADATA_CACHE_CAPACITY must be a number")?;

            config.image_cache_capacity = image_cache_capacity;
        }

        Ok(config)
    }
}

impl Default for WebsiteMetaServiceConfig {
    fn default() -> Self {
        Self {
            metadata_cache_duration: Duration::from_secs(60 * 60 * 48),
            metadata_cache_capacity: 50,
            image_cache_duration: Duration::from_secs(60 * 15),
            image_cache_capacity: 5,
            metadata_connect_timeout: Duration::from_secs(5),
            metadata_read_timeout: Duration::from_secs(10),
        }
    }
}

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
        Self::from_config(Default::default())
    }

    /// Create a web scraper from the provided client
    pub fn from_client(client: reqwest::Client) -> Self {
        Self::from_client_with_config(client, Default::default())
    }

    /// Create a web scraper from the provided config
    pub fn from_config(config: WebsiteMetaServiceConfig) -> reqwest::Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent("DocboxLinkBot")
            .connect_timeout(config.metadata_connect_timeout)
            .read_timeout(config.metadata_read_timeout)
            .build()?;

        Ok(Self::from_client_with_config(client, config))
    }

    /// Create a web scraper from the provided client and config
    pub fn from_client_with_config(
        client: reqwest::Client,
        config: WebsiteMetaServiceConfig,
    ) -> Self {
        // Cache for metadata
        let cache = Cache::builder()
            .time_to_idle(config.metadata_cache_duration)
            .max_capacity(config.metadata_cache_capacity)
            .eviction_policy(EvictionPolicy::tiny_lfu())
            .build();

        // Cache for loaded images
        let image_cache = Cache::builder()
            .time_to_idle(config.image_cache_duration)
            .max_capacity(config.image_cache_capacity)
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
