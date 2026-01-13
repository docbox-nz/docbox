use reqwest::header::{HeaderName, HeaderValue, InvalidHeaderValue};
use thiserror::Error;

use crate::config::ApiConfig;

#[derive(Debug, Error)]
pub enum FlushTenantCacheError {
    #[error(transparent)]
    InvalidHeader(#[from] InvalidHeaderValue),
    #[error(transparent)]
    MakeRequest(#[from] reqwest::Error),
}

/// Makes a request to the docbox API server telling it to flush its
/// database cache
pub async fn flush_tenant_cache(api: &ApiConfig) -> Result<(), FlushTenantCacheError> {
    let client = reqwest::Client::new();

    let url = format!("{}/admin/flush-db-cache", &api.url);
    let mut req_builder = client.post(&url);

    if let Some(api_key) = api.api_key.as_ref() {
        req_builder = req_builder.header(
            HeaderName::from_static("x-docbox-api-key"),
            HeaderValue::from_str(api_key)?,
        );
    }

    let response = req_builder
        .send()
        .await
        .inspect_err(|error| tracing::error!(?error, "failed to request docbox"))?;

    response.error_for_status()?;

    Ok(())
}
