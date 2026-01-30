use crate::links::resolve_website::ResolveWebsiteService;
use docbox_database::{DbPool, models::link::Link};
use docbox_web_scraper::ResolvedWebsiteMetadata;
use thiserror::Error;
use url::Url;

#[derive(Debug, Error)]
pub enum GetLinkMetadataError {
    #[error("failed to parse link url")]
    ParseUrl(#[from] url::ParseError),

    #[error("failed to resolve website metadata")]
    FailedResolve,
}

/// Resolve the metadata for the provided link
#[tracing::instrument(skip_all, fields(link))]
pub async fn get_link_metadata(
    db: &DbPool,
    website_service: &ResolveWebsiteService,
    link: &Link,
) -> Result<(Url, ResolvedWebsiteMetadata), GetLinkMetadataError> {
    let url = Url::parse(&link.value)
        .inspect_err(|error| tracing::warn!(?error, "failed to parse link website"))?;

    let resolved = website_service
        .resolve_website(db, &url)
        .await
        .ok_or_else(|| {
            tracing::warn!("failed to resolve link site metadata");
            GetLinkMetadataError::FailedResolve
        })?;

    Ok((url, resolved))
}
