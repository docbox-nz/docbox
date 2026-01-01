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

        Ok(config)
    }
}

impl Default for CachingWebsiteMetaServiceConfig {
    fn default() -> Self {
        Self {
            metadata_cache_duration: Duration::from_secs(60 * 60 * 48),
            metadata_cache_capacity: 50,
        }
    }
}

/// Wrapper around [WebsiteMetaService] which provides in-memory caching of
/// image and metadata responses
pub struct CachingWebsiteMetaService {
    service: WebsiteMetaService,
    /// Cache for website metadata
    cache: Cache<String, Option<ResolvedWebsiteMetadata>>,
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

        Self { service, cache }
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

        self.service.resolve_image(url, &favicon).await
    }

    /// Resolve the OGP metadata image from the provided URL
    pub async fn resolve_website_image(&self, url: &Url) -> Option<ResolvedImage> {
        let website = self.resolve_website(url).await?;
        let og_image = website.og_image?;

        self.service.resolve_image(url, &og_image).await
    }
}
