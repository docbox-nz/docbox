//! Extractor for extracting the current tenant from the API headers

use std::sync::Arc;

use crate::error::{DynHttpError, HttpCommonError, HttpError};
use axum::{
    Extension,
    extract::{FromRequestParts, Request},
    http::{HeaderMap, StatusCode, request::Parts},
    middleware::Next,
    response::Response,
};
use docbox_core::{
    database::{DatabasePoolCache, DbPool, models::tenant::Tenant},
    events::{EventPublisherFactory, TenantEventPublisher},
    search::{SearchIndexFactory, TenantSearchIndex},
    storage::{StorageLayerFactory, TenantStorageLayer},
    tenant::tenant_cache::TenantCache,
};
use thiserror::Error;
use tracing::Instrument;
use utoipa::IntoParams;
use uuid::Uuid;

// Header for the tenant ID
pub const TENANT_ID_HEADER: &str = "x-tenant-id";
// Header for the tenant env
pub const TENANT_ENV_HEADER: &str = "x-tenant-env";

/// OpenAPI param for requiring the tenant identifier headers
#[derive(IntoParams)]
#[into_params(parameter_in = Header)]
#[allow(unused)]
pub struct TenantParams {
    /// ID of the tenant you are targeting
    #[param(rename = "x-tenant-id")]
    pub tenant_id: String,
    /// Environment of the tenant you are targeting
    #[param(rename = "x-tenant-env")]
    pub tenant_env: String,
}

/// Authenticates the requested tenant, loads the tenant from the database and stores it
/// on the request extensions so it can be extracted by handlers
pub async fn tenant_auth_middleware(
    headers: HeaderMap,
    db_cache: Extension<Arc<DatabasePoolCache>>,
    tenant_cache: Extension<Arc<TenantCache>>,
    mut request: Request,
    next: Next,
) -> Result<Response, DynHttpError> {
    // Extract the request tenant
    let tenant = extract_tenant(&headers, &db_cache, &tenant_cache).await?;

    // Provide a request span that contains the tenant metadata
    let span = tracing::info_span!("tenant", tenant_id = %tenant.id, tenant_env = %tenant.env);

    // Add the tenant as an extension
    request.extensions_mut().insert(tenant);

    // Continue the request normally
    Ok(next.run(request).instrument(span).await)
}

pub fn get_tenant_env(headers: &HeaderMap) -> Result<String, ExtractTenantError> {
    match headers.get(TENANT_ENV_HEADER) {
        Some(value) => value
            .to_str()
            .map_err(|_| ExtractTenantError::InvalidTenantEnv)
            .map(|value| value.to_string()),

        // Tenant not provided
        None => Err(ExtractTenantError::MissingTenantEnv),
    }
}

#[derive(Debug, Error)]
pub enum ExtractTenantError {
    #[error("tenant id is required")]
    MissingTenantId,
    #[error("tenant id must be a valid uuid")]
    InvalidTenantId,
    #[error("tenant env is required")]
    MissingTenantEnv,
    #[error("tenant env must be a valid uuid")]
    InvalidTenantEnv,
    #[error("tenant not found")]
    TenantNotFound,
}

impl HttpError for ExtractTenantError {
    fn status(&self) -> axum::http::StatusCode {
        StatusCode::BAD_REQUEST
    }
}

/// Extracts the target tenant for the provided request
pub async fn extract_tenant(
    headers: &HeaderMap,
    db_cache: &DatabasePoolCache,
    tenant_cache: &TenantCache,
) -> Result<Tenant, DynHttpError> {
    let tenant_id: Uuid = match headers.get(TENANT_ID_HEADER) {
        Some(value) => {
            let value_str = value
                .to_str()
                .map_err(|_| ExtractTenantError::InvalidTenantId)?;

            value_str
                .parse()
                .map_err(|_| ExtractTenantError::InvalidTenantId)?
        }

        // Tenant not provided
        None => return Err(ExtractTenantError::MissingTenantId.into()),
    };

    let env = get_tenant_env(headers)?;

    let db = db_cache.get_root_pool().await.map_err(|error| {
        tracing::error!(?error, "failed to connect to root database");
        HttpCommonError::ServerError
    })?;

    let tenant = tenant_cache
        .get_tenant(&db, env, tenant_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, "failed to query root tenant");
            HttpCommonError::ServerError
        })?
        .ok_or(ExtractTenantError::TenantNotFound)?;

    Ok(tenant)
}

/// Extractor to get database access for the current tenant
pub struct TenantDb(pub DbPool);

impl<S> FromRequestParts<S> for TenantDb
where
    S: Send + Sync,
{
    type Rejection = DynHttpError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // Extract current tenant
        let tenant: &Tenant = parts.extensions.get().ok_or_else(|| {
            tracing::error!("tenant not available within this scope");
            HttpCommonError::ServerError
        })?;

        // Extract database cache
        let db_cache: &Arc<DatabasePoolCache> = parts.extensions.get().ok_or_else(|| {
            tracing::error!("database pool caching is missing");
            HttpCommonError::ServerError
        })?;

        // Create the database connection pool
        let db = db_cache.get_tenant_pool(tenant).await.map_err(|error| {
            tracing::error!(?error, "failed to connect to root database");
            HttpCommonError::ServerError
        })?;

        Ok(TenantDb(db))
    }
}

/// Tenant open search instance
pub struct TenantSearch(pub TenantSearchIndex);

impl<S> FromRequestParts<S> for TenantSearch
where
    S: Send + Sync,
{
    type Rejection = DynHttpError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // Extract current tenant
        let tenant: &Tenant = parts.extensions.get().ok_or_else(|| {
            tracing::error!("tenant not available within this scope");
            HttpCommonError::ServerError
        })?;

        // Extract search index factory
        let factory: &SearchIndexFactory = parts.extensions.get().ok_or_else(|| {
            tracing::error!("search index factory is missing");
            HttpCommonError::ServerError
        })?;

        // Create search index
        let search = factory.create_search_index(tenant);

        Ok(TenantSearch(search))
    }
}

/// Tenant storage access
pub struct TenantStorage(pub TenantStorageLayer);

impl<S> FromRequestParts<S> for TenantStorage
where
    S: Send + Sync,
{
    type Rejection = DynHttpError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // Extract current tenant
        let tenant: &Tenant = parts.extensions.get().ok_or_else(|| {
            tracing::error!("tenant not available within this scope");
            HttpCommonError::ServerError
        })?;

        // Extract open search access
        let factory: &StorageLayerFactory = parts.extensions.get().ok_or_else(|| {
            tracing::error!("storage layer is missing");
            HttpCommonError::ServerError
        })?;

        // Create tenant storage layer
        let storage = factory.create_storage_layer(tenant);

        Ok(TenantStorage(storage))
    }
}

/// Tenant events access
pub struct TenantEvents(pub TenantEventPublisher);

impl<S> FromRequestParts<S> for TenantEvents
where
    S: Send + Sync,
{
    type Rejection = DynHttpError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // Extract current tenant
        let tenant: &Tenant = parts.extensions.get().ok_or_else(|| {
            tracing::error!("tenant not available within this scope");
            HttpCommonError::ServerError
        })?;

        // Get the event publisher factor
        let events: &EventPublisherFactory = parts.extensions.get().ok_or_else(|| {
            tracing::error!("event publisher layer is missing");
            HttpCommonError::ServerError
        })?;

        Ok(TenantEvents(events.create_event_publisher(tenant)))
    }
}
