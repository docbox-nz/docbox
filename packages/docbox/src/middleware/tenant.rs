//! Extractor for extracting the current tenant from the API headers

use std::sync::Arc;

use crate::error::DynHttpError;
use anyhow::{anyhow, Context};
use axum::{
    async_trait,
    extract::{FromRequestParts, Request},
    http::{request::Parts, HeaderMap},
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
use docbox_database::{
    connect_root_database, connect_tenant_database, models::tenant::Tenant, DatabasePoolCache,
    DbPool,
};
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
    let tenant = match extract_tenant(&headers, &db_cache).await {
        Ok(value) => value,
        // Error response
        Err(err) => {
            return Err(DynHttpError::from(err));
        }
    };

    // Add the tenant as an extension
    request.extensions_mut().insert(tenant);

    // Continue the request normally
    Ok(next.run(request).await)
}

pub fn get_tenant_env(headers: &HeaderMap) -> anyhow::Result<String> {
    #[cfg(feature = "mock-browser")]
    {
        return Ok("Development".to_string());
    }

    match headers.get(TENANT_ENV_HEADER) {
        Some(value) => value
            .to_str()
            .context("x-tenant-env was not a valid utf8 string")
            .map(|value| value.to_string()),

        // Tenant not provided
        None => Err(anyhow!("missing {TENANT_ENV_HEADER} header")),
    }
}

/// Extracts the target tenant for the provided request
pub async fn extract_tenant(
    headers: &HeaderMap,
    db_cache: &DatabasePoolCache<AppSecretManager>,
) -> anyhow::Result<Tenant> {
    #[cfg(feature = "mock-browser")]
    let tenant_id = uuid::uuid!("e3bab7bd-07a5-4b81-be38-e4790e80c0d1");
    #[cfg(feature = "mock-browser")]
    let env = "Development";

    #[cfg(not(feature = "mock-browser"))]
    let tenant_id: Uuid = match headers.get(TENANT_ID_HEADER) {
        Some(value) => {
            let value_str = value
                .to_str()
                .context("x-tenant-id was not a valid utf8 string")?;

            value_str
                .parse()
                .context("tenant id must be a valid uuid")?
        }

        // Tenant not provided
        None => return Err(anyhow!("missing {TENANT_ID_HEADER} header")),
    };

    #[cfg(not(feature = "mock-browser"))]
    let env = get_tenant_env(headers)?;

    let db = connect_root_database(db_cache).await?;

    let tenant = Tenant::find_by_id(&db, tenant_id, &env)
        .await
        .context("failed to request tenant")?
        .context("tenant does not exist")?;

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
        let tenant: &Tenant = parts
            .extensions
            .get()
            .context("tenant not available within this scope")?;

        // Extract database cache
        let db_cache: &Arc<DatabasePoolCache<AppSecretManager>> = parts
            .extensions
            .get()
            .context("missing tenant database cache")?;

        // Create the database connection pool
        let db = connect_tenant_database(db_cache, tenant).await?;

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
        let tenant: &Tenant = parts
            .extensions
            .get()
            .context("tenant not available within this scope")?;

        // Extract search index factory
        let factory: &SearchIndexFactory = parts
            .extensions
            .get()
            .context("search index factory missing")?;

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
        let tenant: &Tenant = parts
            .extensions
            .get()
            .context("tenant not available within this scope")?;

        // Extract open search access
        let factory: &StorageLayerFactory = parts.extensions.get().context("s3 client missing")?;

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
        let tenant: &Tenant = parts
            .extensions
            .get()
            .context("tenant not available within this scope")?;

        // Get the event publisher factor
        let events: &EventPublisherFactory =
            parts.extensions.get().context("sqs client missing")?;

        Ok(TenantEvents(events.create_event_publisher(tenant)))
    }
}
