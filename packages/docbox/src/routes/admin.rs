//! Admin related access and routes for managing tenants and document boxes

use crate::{
    error::{HttpCommonError, HttpErrorResponse, HttpResult, HttpStatusResult},
    middleware::tenant::{TenantDb, TenantParams, TenantSearch},
};
use axum::{http::StatusCode, Extension, Json};
use axum_valid::Garde;
use docbox_core::{
    search::{
        models::{
            AdminSearchRequest, AdminSearchResultResponse, FlattenedItemResult, SearchResultData,
            SearchResultItem,
        },
        os::resolve_search_result,
    },
    secrets::AppSecretManager,
    services::files::presigned::purge_expired_presigned_tasks,
    storage::StorageLayerFactory,
};
use docbox_database::{
    models::{document_box::WithScope, folder::FolderPathSegment},
    DatabasePoolCache,
};
use std::sync::Arc;
use tracing::error;

pub const ADMIN_TAG: &str = "Admin";

/// Admin Search
///
/// Performs a search across multiple document box scopes. This
/// is an administrator route as unlike other routes we cannot
/// assert through the URL that the user has access to all the
/// scopes
#[utoipa::path(
    post,
    operation_id = "admin_search_tenant",
    tag = ADMIN_TAG,
    path = "/admin/search",
    responses(
        (status = 201, description = "Searched successfully", body = AdminSearchResultResponse),
        (status = 400, description = "Malformed or invalid request not meeting validation requirements", body = HttpErrorResponse),
        (status = 409, description = "Scope already exists", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(TenantParams)
)]
#[tracing::instrument(skip_all, fields(req))]
pub async fn search_tenant(
    TenantDb(db): TenantDb,
    TenantSearch(search): TenantSearch,
    Garde(Json(req)): Garde<Json<AdminSearchRequest>>,
) -> HttpResult<AdminSearchResultResponse> {
    // Not searching any scopes
    if req.scopes.is_empty() {
        return Ok(Json(AdminSearchResultResponse {
            total_hits: 0,
            results: vec![],
        }));
    }

    let results = search.search_index(&req.scopes, req.request, None).await?;

    let mut resolved: Vec<(
        FlattenedItemResult,
        SearchResultData,
        Vec<FolderPathSegment>,
    )> = Vec::with_capacity(results.results.len());

    for result in results.results {
        match resolve_search_result(&db, result).await {
            Ok(value) => resolved.push(value),
            Err(cause) => {
                error!(?cause, "failed to fetch search result from database");
                continue;
            }
        }
    }

    let out: Vec<WithScope<SearchResultItem>> = resolved
        .into_iter()
        .map(|(hit, data, path)| WithScope {
            data: SearchResultItem {
                path,
                score: hit.score,
                data,
                page_matches: hit.page_matches,
                total_hits: hit.total_hits,
                name_match: hit.name_match,
                content_match: hit.content_match,
            },
            scope: hit.document_box,
        })
        .collect();

    Ok(Json(AdminSearchResultResponse {
        total_hits: results.total_hits,
        results: out,
    }))
}

/// Flush database cache
///
/// Empties all the database pool and credentials caches, you can use this endpoint
/// if you rotate your database credentials to refresh the database pool without
/// needing to restart the server
#[utoipa::path(
    post,
    operation_id = "admin_flush_database_pool_cache",
    tag = ADMIN_TAG,
    path = "/admin/flush-db-cache",
    responses(
        (status = 204, description = "Database cache flushed"),
    )
)]
pub async fn flush_database_pool_cache(
    Extension(db_cache): Extension<Arc<DatabasePoolCache<AppSecretManager>>>,
) -> HttpStatusResult {
    db_cache.flush().await;
    Ok(StatusCode::NO_CONTENT)
}

/// Purge Presigned Tasks
///
/// Purges all expired presigned tasks
#[utoipa::path(
    post,
    operation_id = "admin_purge_expired_presigned_tasks",
    tag = ADMIN_TAG,
    path = "/admin/purge-expired-presigned-tasks",
    responses(
        (status = 204, description = "Database cache flushed"),
        (status = 500, description = "Failed to purge presigned cache", body = HttpErrorResponse),
    )
)]
pub async fn http_purge_expired_presigned_tasks(
    Extension(db_cache): Extension<Arc<DatabasePoolCache<AppSecretManager>>>,
    Extension(storage_factory): Extension<StorageLayerFactory>,
) -> HttpStatusResult {
    purge_expired_presigned_tasks(db_cache, storage_factory)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to purge expired presigned tasks");
            HttpCommonError::ServerError
        })?;

    Ok(StatusCode::NO_CONTENT)
}
