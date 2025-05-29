//! Admin related access and routes for managing tenants and document boxes

use crate::{
    error::{HttpCommonError, HttpErrorResponse, HttpResult, HttpStatusResult},
    middleware::tenant::{TenantDb, TenantParams, TenantSearch},
};
use axum::{http::StatusCode, Extension, Json};
use axum_valid::Garde;
use docbox_core::{
    document_box::search_document_box::{search_document_boxes_admin, ResolvedSearchResult},
    files::upload_file_presigned::purge_expired_presigned_tasks,
    search::models::{AdminSearchRequest, AdminSearchResultResponse, SearchResultItem},
    secrets::AppSecretManager,
    storage::StorageLayerFactory,
};
use docbox_database::{models::document_box::WithScope, DatabasePoolCache};
use std::sync::Arc;

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
#[tracing::instrument(skip_all, fields(req = ?req))]
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

    let resolved = search_document_boxes_admin(&db, &search, req)
        .await
        .map_err(|error| {
            tracing::error!(?error, "failed to perform admin search");
            HttpCommonError::ServerError
        })?;

    let out: Vec<WithScope<SearchResultItem>> = resolved
        .results
        .into_iter()
        .map(|ResolvedSearchResult { result, data, path }| WithScope {
            data: SearchResultItem {
                path,
                score: result.score,
                data,
                page_matches: result.page_matches,
                total_hits: result.total_hits,
                name_match: result.name_match,
                content_match: result.content_match,
            },
            scope: result.document_box,
        })
        .collect();

    Ok(Json(AdminSearchResultResponse {
        total_hits: resolved.total_hits,
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
