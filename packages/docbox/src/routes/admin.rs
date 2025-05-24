//! Admin related access and routes for managing tenants and document boxes

use crate::{
    error::{HttpResult, HttpStatusResult},
    middleware::tenant::{TenantDb, TenantSearch},
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

/// POST /admin/search
///
/// Gets a tenant by ID responding with all the document
/// boxes in the container
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

/// POST /admin/flush-db-cache
///
/// Empties all the database pool and credentials caches
pub async fn flush_database_pool_cache(
    Extension(db_cache): Extension<Arc<DatabasePoolCache<AppSecretManager>>>,
) -> HttpStatusResult {
    db_cache.flush().await;
    Ok(StatusCode::NO_CONTENT)
}

/// POST /admin/purge-expired-presigned-tasks
///
/// Purges all expired presigned tasks
pub async fn http_purge_expired_presigned_tasks(
    Extension(db_cache): Extension<Arc<DatabasePoolCache<AppSecretManager>>>,
    Extension(storage_factory): Extension<StorageLayerFactory>,
) -> HttpStatusResult {
    purge_expired_presigned_tasks(db_cache, storage_factory).await?;
    Ok(StatusCode::NO_CONTENT)
}
