use crate::{ResolvedImage, ResolvedWebsiteMetadata, WebsiteMetaService};
use moka::{future::Cache, policy::EvictionPolicy};
use serde::{Deserialize, Serialize};
use std::{str::FromStr, time::Duration};
use thiserror::Error;
use tracing::Instrument;
use url::Url;

/// Configuration for caching data in the website metadata service cache
#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct CachingWebsiteMetaServiceConfig {
    /// Duration to maintain site metadata for
    ///
    /// Default: 48h
    pub metadata_cache_duration: Duration,
    /// Maximum number of site metadata to maintain in the cache
    ///
    /// Default: 50
    pub metadata_cache_capacity: u64,

    /// Duration to maintain resolved images for
    ///
    /// Default: 15min
    pub image_cache_duration: Duration,
    /// Maximum number of images to maintain in the cache
    ///
    /// Default: 5
    pub image_cache_capacity: u64,
}

/// Errors that could occur when loading the configuration
#[derive(Debug, Error)]
pub enum CachingWebsiteMetaServiceConfigError {
    /// Provided cache duration was an invalid number
    #[error("DOCBOX_WEB_SCRAPE_METADATA_CACHE_DURATION must be a number in seconds: {0}")]
    InvalidMetadataCacheDuration(<u64 as FromStr>::Err),
    /// Provided cache capacity was an invalid number
    #[error("DOCBOX_WEB_SCRAPE_METADATA_CACHE_CAPACITY must be a number: {0}")]
    InvalidMetadataCacheCapacity(<u64 as FromStr>::Err),
    /// Provided image cache duration was an invalid number
    #[error("DOCBOX_WEB_SCRAPE_IMAGE_CACHE_DURATION must be a number in seconds")]
    InvalidImageCacheDuration(<u64 as FromStr>::Err),
    /// Provided image cache capacity was an invalid number
    #[error("DOCBOX_WEB_SCRAPE_IMAGE_CACHE_CAPACITY must be a number")]
    InvalidImageCacheCapacity(<u64 as FromStr>::Err),
}

impl CachingWebsiteMetaServiceConfig {
    /// Load a website meta service config from its environment variables
    pub fn from_env()
    -> Result<CachingWebsiteMetaServiceConfig, CachingWebsiteMetaServiceConfigError> {
        let mut config = CachingWebsiteMetaServiceConfig::default();

        if let Ok(metadata_cache_duration) =
            std::env::var("DOCBOX_WEB_SCRAPE_METADATA_CACHE_DURATION")
        {
            let metadata_cache_duration = metadata_cache_duration
                .parse::<u64>()
                .map_err(CachingWebsiteMetaServiceConfigError::InvalidMetadataCacheDuration)?;

            config.metadata_cache_duration = Duration::from_secs(metadata_cache_duration);
        }

        if let Ok(metadata_cache_capacity) =
            std::env::var("DOCBOX_WEB_SCRAPE_METADATA_CACHE_CAPACITY")
        {
            let metadata_cache_capacity = metadata_cache_capacity
                .parse::<u64>()
                .map_err(CachingWebsiteMetaServiceConfigError::InvalidMetadataCacheCapacity)?;

            config.metadata_cache_capacity = metadata_cache_capacity;
        }

        if let Ok(image_cache_duration) = std::env::var("DOCBOX_WEB_SCRAPE_IMAGE_CACHE_DURATION") {
            let image_cache_duration = image_cache_duration
                .parse::<u64>()
                .map_err(CachingWebsiteMetaServiceConfigError::InvalidImageCacheDuration)?;

            config.image_cache_duration = Duration::from_secs(image_cache_duration);
        }

        if let Ok(image_cache_capacity) = std::env::var("DOCBOX_WEB_SCRAPE_IMAGE_CACHE_CAPACITY") {
            let image_cache_capacity = image_cache_capacity
                .parse::<u64>()
                .map_err(CachingWebsiteMetaServiceConfigError::InvalidImageCacheCapacity)?;

            config.image_cache_capacity = image_cache_capacity;
        }

        Ok(config)
    }
}

impl Default for CachingWebsiteMetaServiceConfig {
    fn default() -> Self {
        Self {
            metadata_cache_duration: Duration::from_secs(60 * 60 * 48),
            metadata_cache_capacity: 50,
            image_cache_duration: Duration::from_secs(60 * 15),
            image_cache_capacity: 5,
        }
    }
}

/// Wrapper around [WebsiteMetaService] which provides in-memory caching of
/// image and metadata responses
pub struct CachingWebsiteMetaService {
    service: WebsiteMetaService,
    /// Cache for website metadata
    cache: Cache<String, Option<ResolvedWebsiteMetadata>>,
    /// Cache for resolved images will contain [None] for images that failed to load
    image_cache: Cache<(String, ImageCacheKey), Option<ResolvedImage>>,
}

/// Cache key for image cache value types
#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
enum ImageCacheKey {
    Favicon,
    Image,
}

impl CachingWebsiteMetaService {
    /// Exchange the caching service for the underlying meta service
    pub fn into_inner(self) -> WebsiteMetaService {
        self.service
    }

    /// Create a new caching website metadata service
    pub fn from_client_with_config(
        service: WebsiteMetaService,
        config: CachingWebsiteMetaServiceConfig,
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
            service,
            cache,
            image_cache,
        }
    }

    /// Resolves the metadata for the website at the provided URL
    pub async fn resolve_website(&self, url: &Url) -> Option<ResolvedWebsiteMetadata> {
        let span = tracing::Span::current();
        let inner = self.service.resolve_website(url);
        self.cache
            .get_with(url.to_string(), inner.instrument(span))
            .await
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

        self.resolve_image(url, ImageCacheKey::Favicon, favicon)
            .await
    }

    /// Resolve the OGP metadata image from the provided URL
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
        let span = tracing::Span::current();
        let inner = self.service.resolve_image(url, &image);

        self.image_cache
            .get_with((url.to_string(), cache_key), inner.instrument(span))
            .await
    }
}
