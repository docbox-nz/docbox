//! Extractor for extracting the current tenant from the API headers

use std::sync::Arc;

use crate::error::{DynHttpError, HttpCommonError, HttpError};
use axum::{
    async_trait,
    extract::{FromRequestParts, Request},
    http::{request::Parts, HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
    Extension,
};
use docbox_core::{
    events::{EventPublisherFactory, TenantEventPublisher},
    search::{SearchIndexFactory, TenantSearchIndex},
    secrets::AppSecretManager,
    storage::{StorageLayerFactory, TenantStorageLayer},
};
use docbox_database::{models::tenant::Tenant, DatabasePoolCache, DbPool};
use thiserror::Error;
use utoipa::IntoParams;
use uuid::Uuid;

// Header for the tenant ID
const TENANT_ID_HEADER: &str = "x-tenant-id";
// Header for the tenant env
const TENANT_ENV_HEADER: &str = "x-tenant-env";

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
    db_cache: Extension<Arc<DatabasePoolCache<AppSecretManager>>>,
    mut request: Request,
    next: Next,
) -> Result<Response, DynHttpError> {
    // Extract the request tenant
    let tenant = extract_tenant(&headers, &db_cache).await?;

    // Add the tenant as an extension
    request.extensions_mut().insert(tenant);

    // Continue the request normally
    Ok(next.run(request).await)
}

pub fn get_tenant_env(headers: &HeaderMap) -> Result<String, ExtractTenantError> {
    #[cfg(feature = "mock-browser")]
    {
        return Ok("Development".to_string());
    }

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
    db_cache: &DatabasePoolCache<AppSecretManager>,
) -> Result<Tenant, DynHttpError> {
    #[cfg(feature = "mock-browser")]
    let tenant_id = uuid::uuid!("e3bab7bd-07a5-4b81-be38-e4790e80c0d1");
    #[cfg(feature = "mock-browser")]
    let env = "Development";

    #[cfg(not(feature = "mock-browser"))]
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

    #[cfg(not(feature = "mock-browser"))]
    let env = get_tenant_env(headers)?;

    let db = db_cache.get_root_pool().await.map_err(|cause| {
        tracing::error!(?cause, "failed to connect to root database");
        HttpCommonError::ServerError
    })?;

    let tenant = Tenant::find_by_id(&db, tenant_id, &env)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to connect to root database");
            HttpCommonError::ServerError
        })?
        .ok_or(ExtractTenantError::TenantNotFound)?;

    Ok(tenant)
}

/// Extractor to get database access for the current tenant
pub struct TenantDb(pub DbPool);

#[async_trait]
impl<S> FromRequestParts<S> for TenantDb
where
    S: Send + 'static,
{
    type Rejection = DynHttpError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // Extract current tenant
        let tenant: &Tenant = parts.extensions.get().ok_or_else(|| {
            tracing::error!("tenant not available within this scope");
            HttpCommonError::ServerError
        })?;

        // Extract database cache
        let db_cache: &Arc<DatabasePoolCache<AppSecretManager>> =
            parts.extensions.get().ok_or_else(|| {
                tracing::error!("database pool caching is missing");
                HttpCommonError::ServerError
            })?;

        // Create the database connection pool
        let db = db_cache.get_tenant_pool(tenant).await.map_err(|cause| {
            tracing::error!(?cause, "failed to connect to root database");
            HttpCommonError::ServerError
        })?;

        Ok(TenantDb(db))
    }
}

/// Tenant open search instance
pub struct TenantSearch(pub TenantSearchIndex);

#[async_trait]
impl<S> FromRequestParts<S> for TenantSearch
where
    S: Send + 'static,
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

/// Tenant S3 access
pub struct TenantStorage(pub TenantStorageLayer);

#[async_trait]
impl<S> FromRequestParts<S> for TenantStorage
where
    S: Send + 'static,
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

/// Tenant S3 access
pub struct TenantEvents(pub TenantEventPublisher);

#[async_trait]
impl<S> FromRequestParts<S> for TenantEvents
where
    S: Send + 'static,
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
