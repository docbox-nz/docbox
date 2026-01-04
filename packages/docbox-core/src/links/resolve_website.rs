use chrono::{TimeDelta, Utc};
use docbox_database::{
    DbPool,
    models::link_resolved_metadata::{
        CreateLinkResolvedMetadata, LinkResolvedMetadata, StoredResolvedWebsiteMetadata,
    },
};
use docbox_web_scraper::{ResolvedWebsiteMetadata, WebsiteMetaService};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, str::FromStr, sync::Arc};
use thiserror::Error;
use tokio::sync::Mutex;
use url::Url;

/// Configuration for caching data in the website metadata service cache
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ResolveWebsiteConfig {
    /// Duration to maintain site metadata for
    ///
    /// Default: 48h
    pub metadata_cache_duration: TimeDelta,
}

impl Default for ResolveWebsiteConfig {
    fn default() -> Self {
        Self {
            metadata_cache_duration: TimeDelta::hours(48),
        }
    }
}

/// Errors that could occur when loading the configuration
#[derive(Debug, Error)]
pub enum ResolveWebsiteConfigError {
    /// Provided cache duration was an invalid number
    #[error("DOCBOX_WEB_SCRAPE_METADATA_CACHE_DURATION must be a number in seconds: {0}")]
    InvalidMetadataCacheDuration(<i64 as FromStr>::Err),

    /// Provided cache duration was not within the allowed bounds
    #[error("DOCBOX_WEB_SCRAPE_METADATA_CACHE_DURATION must be within the valid seconds bounds")]
    MetadataCacheDurationOutOfBounds,
}

impl ResolveWebsiteConfig {
    /// Load a website meta service config from its environment variables
    pub fn from_env() -> Result<ResolveWebsiteConfig, ResolveWebsiteConfigError> {
        let mut config = ResolveWebsiteConfig::default();

        if let Ok(metadata_cache_duration) =
            std::env::var("DOCBOX_WEB_SCRAPE_METADATA_CACHE_DURATION")
        {
            let metadata_cache_duration = metadata_cache_duration
                .parse::<i64>()
                .map_err(ResolveWebsiteConfigError::InvalidMetadataCacheDuration)?;

            // Prevent panic by ensuring value range
            if !(-i64::MAX / 1_000..i64::MAX / 1_000).contains(&metadata_cache_duration) {
                return Err(ResolveWebsiteConfigError::MetadataCacheDurationOutOfBounds);
            }

            config.metadata_cache_duration = TimeDelta::seconds(metadata_cache_duration);
        }

        Ok(config)
    }
}

pub struct ResolveWebsiteService {
    pub service: WebsiteMetaService,
    config: ResolveWebsiteConfig,

    /// Lock for concurrent requests to prevent duplicate fetching
    locks: RequestLock,
}

/// Simple per-url lock system
#[derive(Default)]
struct RequestLock {
    locks: Mutex<HashMap<Url, Arc<Mutex<()>>>>,
}

impl RequestLock {
    pub async fn acquire(&self, url: &Url) -> Arc<Mutex<()>> {
        let mut locks = self.locks.lock().await;
        locks.entry(url.to_owned()).or_default().clone()
    }

    pub async fn remove(&self, url: &Url) {
        let mut locks = self.locks.lock().await;
        locks.remove(url);
    }
}

impl ResolveWebsiteService {
    /// Create a new [ResolveWebsiteService] from the provided `service` and `config`
    pub fn from_client_with_config(
        service: WebsiteMetaService,
        config: ResolveWebsiteConfig,
    ) -> Self {
        Self {
            service,
            config,
            locks: Default::default(),
        }
    }

    /// Resolves the metadata for the website at the provided URL
    pub async fn resolve_website(&self, db: &DbPool, url: &Url) -> Option<ResolvedWebsiteMetadata> {
        // Check the database for existing metadata
        if let Some(value) = self.resolve_website_db(db, url).await {
            return Some(value);
        }

        // Acquire lock before attempting to resolve
        let lock = self.locks.acquire(url).await;
        let _guard = lock.lock().await;

        // Re-check the database for existing metadata in-case someone else resolved the data
        if let Some(value) = self.resolve_website_db(db, url).await {
            // Remove the lock in-case we added a lock
            self.locks.remove(url).await;
            return Some(value);
        }

        // Resolve the metadata
        let resolved = self.service.resolve_website(url).await;
        if let Some(resolved) = resolved.as_ref() {
            // Persist the resolved metadata to the database
            self.persist_resolved_metadata(db, url.as_str(), resolved)
                .await;
        }

        self.locks.remove(url).await;
        resolved
    }

    /// Query the database for resolved link metadata
    async fn resolve_website_db(&self, db: &DbPool, url: &Url) -> Option<ResolvedWebsiteMetadata> {
        if let Some(resolved) = LinkResolvedMetadata::query(db, url.as_str())
            .await
            .inspect_err(|error| tracing::error!(?error, "failed to query link resolved metadata"))
            .ok()?
        {
            // Ensure the resolved data is not expired
            let now = Utc::now();
            if resolved.expires_at > now {
                let metadata = resolved.metadata;
                return Some(ResolvedWebsiteMetadata {
                    title: metadata.title,
                    og_title: metadata.og_title,
                    og_description: metadata.og_description,
                    og_image: metadata.og_image,
                    best_favicon: metadata.best_favicon,
                });
            }
        }

        None
    }

    /// Store resolved link metadata in the database
    async fn persist_resolved_metadata(
        &self,
        db: &DbPool,
        url: &str,
        resolved: &ResolvedWebsiteMetadata,
    ) {
        let now = Utc::now();
        let expires_at = match now.checked_add_signed(self.config.metadata_cache_duration) {
            Some(value) => value,
            None => {
                tracing::error!("failed to compute expires at date, time computation overflowed");
                return;
            }
        };

        // Persist the resolved metadata to the database
        if let Err(error) = LinkResolvedMetadata::create(
            db,
            CreateLinkResolvedMetadata {
                url: url.to_string(),
                metadata: StoredResolvedWebsiteMetadata {
                    title: resolved.title.clone(),
                    og_title: resolved.og_title.clone(),
                    og_description: resolved.og_description.clone(),
                    og_image: resolved.og_image.clone(),
                    best_favicon: resolved.best_favicon.clone(),
                },
                expires_at,
            },
        )
        .await
        {
            tracing::error!(?error, "failed to store resolved link metadata")
        }
    }
}
