//! Admin related access and routes for managing tenants and document boxes

use crate::{
    error::{HttpCommonError, HttpErrorResponse, HttpResult, HttpStatusResult},
    middleware::tenant::{TenantDb, TenantParams, TenantSearch, TenantStorage},
    models::admin::{TenantDocumentBoxesRequest, TenantDocumentBoxesResponse},
};
use axum::{Extension, Json, http::StatusCode};
use axum_valid::Garde;
use docbox_core::{
    document_box::search_document_box::{ResolvedSearchResult, search_document_boxes_admin},
    files::purge_expired_presigned_tasks::purge_expired_presigned_tasks,
    processing::ProcessingLayer,
    tenant::tenant_cache::TenantCache,
};
use docbox_database::{
    DatabasePoolCache,
    models::document_box::{DocumentBox, WithScope},
};
use docbox_search::models::{AdminSearchRequest, AdminSearchResultResponse, SearchResultItem};
use docbox_storage::StorageLayerFactory;
use std::sync::Arc;

pub const ADMIN_TAG: &str = "Admin";

/// Admin Boxes
///
/// Requests a list of document boxes within the tenant
#[utoipa::path(
    post,
    operation_id = "admin_tenant_boxes",
    tag = ADMIN_TAG,
    path = "/admin/boxes",
    responses(
        (status = 201, description = "Searched successfully", body = TenantDocumentBoxesResponse),
        (status = 400, description = "Malformed or invalid request not meeting validation requirements", body = HttpErrorResponse),
        (status = 409, description = "Scope already exists", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(TenantParams)
)]
#[tracing::instrument(skip_all, fields(req = ?req))]
pub async fn tenant_boxes(
    TenantDb(db): TenantDb,
    Garde(Json(req)): Garde<Json<TenantDocumentBoxesRequest>>,
) -> HttpResult<TenantDocumentBoxesResponse> {
    let offset = req.offset.unwrap_or(0);
    let limit = req.size.unwrap_or(100);

    let (document_boxes, total) = match req.query {
        Some(query) if !query.is_empty() => {
            // Adjust the query to be better suited for searching
            let mut query = query
                // Replace wildcards with the SQL wildcard version
                .replace("*", "%")
                // Escape underscore literal
                .replace("_", "\\_");

            let has_wildcard = query.chars().any(|char| matches!(char, '*' | '%'));

            // Query contains no wildcards, insert a wildcard at the end for prefix matching
            if !has_wildcard {
                query.push('%');
            }

            let document_boxes = DocumentBox::search_query(&db, &query, offset, limit as u64)
                .await
                .map_err(|error| {
                    tracing::error!(?error, "failed to query document boxes");
                    HttpCommonError::ServerError
                })?;

            let total = DocumentBox::search_total(&db, &query)
                .await
                .map_err(|error| {
                    tracing::error!(?error, "failed to query document boxes total");
                    HttpCommonError::ServerError
                })?;

            (document_boxes, total)
        }
        _ => {
            let document_boxes = DocumentBox::query(&db, offset, limit as u64)
                .await
                .map_err(|error| {
                    tracing::error!(?error, "failed to query document boxes");
                    HttpCommonError::ServerError
                })?;

            let total = DocumentBox::total(&db).await.map_err(|error| {
                tracing::error!(?error, "failed to query document boxes total");
                HttpCommonError::ServerError
            })?;

            (document_boxes, total)
        }
    };

    Ok(Json(TenantDocumentBoxesResponse {
        results: document_boxes,
        total,
    }))
}

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

/// Reprocess octet-stream files
///
/// Useful if a files were previously accepted into the tenant with some unknown
/// file type (or ingested through a source that was unable to get the correct mime).
///
/// Will reprocess files that have this unknown file type mime to see if a different
/// type can be obtained
#[utoipa::path(
    post,
    operation_id = "admin_reprocess_octet_stream_files",
    tag = ADMIN_TAG,
    path = "/admin/reprocess-octet-stream-files",
    responses(
        (status = 204, description = "Reprocessed successfully", body = AdminSearchResultResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(TenantParams)
)]
#[tracing::instrument(skip_all)]
pub async fn reprocess_octet_stream_files_tenant(
    TenantDb(db): TenantDb,
    TenantSearch(search): TenantSearch,
    TenantStorage(storage): TenantStorage,
    Extension(processing): Extension<ProcessingLayer>,
) -> HttpStatusResult {
    docbox_core::files::reprocess_octet_stream_files::reprocess_octet_stream_files(
        &db,
        &search,
        &storage,
        &processing,
    )
    .await
    .map_err(|error| {
        tracing::error!(?error, "failed to reprocess octet-stream files");
        HttpCommonError::ServerError
    })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Rebuild search index
///
/// Rebuild the tenant search index from the data stored in the database
/// and in storage
#[utoipa::path(
    post,
    operation_id = "admin_rebuild_search_index",
    tag = ADMIN_TAG,
    path = "/admin/rebuild-search-index",
    responses(
        (status = 204, description = "Rebuilt successfully", body = AdminSearchResultResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(TenantParams)
)]
#[tracing::instrument(skip_all)]
pub async fn rebuild_search_index_tenant(
    TenantDb(db): TenantDb,
    TenantSearch(search): TenantSearch,
    TenantStorage(storage): TenantStorage,
) -> HttpStatusResult {
    docbox_core::tenant::rebuild_tenant_index::rebuild_tenant_index(&db, &search, &storage)
        .await
        .map_err(|error| {
            tracing::error!(?error, "failed to rebuilt tenant search index");
            HttpCommonError::ServerError
        })?;

    Ok(StatusCode::NO_CONTENT)
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
    Extension(db_cache): Extension<Arc<DatabasePoolCache>>,
) -> HttpStatusResult {
    db_cache.flush().await;
    Ok(StatusCode::NO_CONTENT)
}

/// Flush tenant cache
///
/// Clears the tenant cache, you can use this endpoint if you've updated the
/// tenant configuration and want it to be applied immediately without
/// restarting the server
#[utoipa::path(
    post,
    operation_id = "admin_flush_tenant_cache",
    tag = ADMIN_TAG,
    path = "/admin/flush-tenant-cache",
    responses(
        (status = 204, description = "Tenant cache flushed"),
    )
)]
pub async fn flush_tenant_cache(
    Extension(tenant_cache): Extension<Arc<TenantCache>>,
) -> HttpStatusResult {
    tenant_cache.flush().await;
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
    Extension(db_cache): Extension<Arc<DatabasePoolCache>>,
    Extension(storage_factory): Extension<StorageLayerFactory>,
) -> HttpStatusResult {
    purge_expired_presigned_tasks(db_cache, storage_factory)
        .await
        .map_err(|_| HttpCommonError::ServerError)?;

    Ok(StatusCode::NO_CONTENT)
}
