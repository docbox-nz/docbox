//! Admin related access and routes for managing tenants and document boxes

use crate::{
    error::{DynHttpError, HttpResult, HttpStatusResult},
    middleware::tenant::{get_tenant_env, TenantDb, TenantSearch},
    models::admin::{CreateTenant, CreatedTenant, HttpTenantError, TenantResponse},
};
use anyhow::Context;
use axum::{
    extract::Path,
    http::{HeaderMap, StatusCode},
    Extension, Json,
};
use axum_valid::Garde;
use docbox_core::{
    search::{
        models::{
            AdminSearchRequest, AdminSearchResultResponse, FlattenedItemResult, SearchResultData,
            SearchResultItem,
        },
        os::resolve_search_result,
        SearchIndexFactory,
    },
    secrets::AppSecretManager,
    services::{
        files::presigned::purge_expired_presigned_tasks,
        tenant::{initialize_tenant, rollback_tenant_error, InitTenantState},
    },
    storage::StorageLayerFactory,
};
use docbox_database::{
    connect_root_database, connect_tenant_database,
    models::{
        document_box::{DocumentBox, WithScope},
        folder::FolderPathSegment,
        tenant::{Tenant, TenantId},
    },
    DatabasePoolCache,
};
use std::sync::Arc;
use tracing::error;

/// POST /admin/tenant
///
/// Creates a new tenant within the doc-box and creates the tables
/// for the tenant
pub async fn create_tenant(
    headers: HeaderMap,
    Extension(db_cache): Extension<Arc<DatabasePoolCache<AppSecretManager>>>,
    Extension(search_factory): Extension<SearchIndexFactory>,
    Extension(storage_factory): Extension<StorageLayerFactory>,
    Garde(Json(create)): Garde<Json<CreateTenant>>,
) -> Result<(StatusCode, Json<CreatedTenant>), DynHttpError> {
    let env = get_tenant_env(&headers)?;
    let mut init_state = InitTenantState::default();

    let tenant = match initialize_tenant(
        &db_cache,
        &search_factory,
        &storage_factory,
        docbox_core::services::tenant::CreateTenant {
            id: create.id,
            db_name: create.db_name,
            db_secret_name: create.db_secret_name,
            s3_name: create.s3_name,
            os_index_name: create.os_index_name,
            event_queue_url: create.event_queue_url,
            origins: create.origins,
            s3_queue_arn: create.s3_queue_arn,
        },
        env,
        &mut init_state,
    )
    .await
    {
        Ok(value) => value,
        Err(err) => {
            // Attempt to rollback any allocated resources in the background
            tokio::spawn(async move {
                rollback_tenant_error(init_state).await;
            });

            return Err(anyhow::Error::from(err).into());
        }
    };

    Ok((StatusCode::CREATED, Json(CreatedTenant { id: tenant.id })))
}

/// GET /admin/tenant/:tenant-id
///
/// Gets a tenant by ID responding with all the document
/// boxes in the container
pub async fn get_tenant(
    headers: HeaderMap,
    Path(tenant_id): Path<TenantId>,
    Extension(db_cache): Extension<Arc<DatabasePoolCache<AppSecretManager>>>,
) -> HttpResult<TenantResponse> {
    let env = get_tenant_env(&headers)?;

    let db = connect_root_database(&db_cache).await?;

    let tenant = Tenant::find_by_id(&db, tenant_id, &env)
        .await
        .context("failed to request tenant")?
        .ok_or(HttpTenantError::UnknownTenant)?;

    drop(db);

    let db = connect_tenant_database(&db_cache, &tenant).await?;

    let document_boxes = DocumentBox::all(&db)
        .await
        .context("failed to query tenant document boxes")?;

    Ok(Json(TenantResponse {
        tenant,
        document_boxes,
    }))
}

/// DELETE /admin/tenant/:tenant-id
///
/// Deletes a tenant by ID, will delete all contained document boxes and files
///
/// Could take some time, ensure to make sure you disable request timeouts
/// when using this.
pub async fn delete_tenant(
    headers: HeaderMap,
    Path(tenant_id): Path<TenantId>,
    Extension(db_cache): Extension<Arc<DatabasePoolCache<AppSecretManager>>>,
    // Extension(opensearch): Extension<OpenSearch>,
    // Extension(s3): Extension<S3Client>,
) -> HttpStatusResult {
    let env = get_tenant_env(&headers)?;
    let db = connect_root_database(&db_cache).await?;

    let tenant = Tenant::find_by_id(&db, tenant_id, &env)
        .await
        .context("failed to request tenant")?
        .ok_or(HttpTenantError::UnknownTenant)?;

    // let tenant_db = connect_tenant_database(&db_cache, &tenant).await?;

    // let document_boxes = DocumentBox::all(&tenant_db)
    //     .await
    //     .context("failed to query tenant document boxes")?;

    // let tenant_bucket = TenantBucket::from_tenant(&tenant);
    // let tenant_index = TenantSearchIndex::from_tenant(&tenant);
    // let opensearch = TenantOpenSearch::new(opensearch, tenant_index);
    // let s3 = TenantS3Client::new(s3, tenant_bucket);

    // // Delete all document boxes in the tenant
    // for document_box in document_boxes {
    //     let root = Folder::find_root(&tenant_db, &document_box.scope).await?;

    //     if let Some(root) = root {
    //         // Delete root folder
    //         delete_folder(&tenant_db, &s3, &opensearch, root)
    //             .await
    //             .context("failed to delete bucket root folder")?;
    //     }

    //     // Delete document box
    //     document_box
    //         .delete(&tenant_db)
    //         .await
    //         .context("failed to delete document box")?;
    // }

    // s3.delete_bucket()
    //     .await
    //     .context("failed to delete s3 bucket")?;

    // opensearch
    //     .delete_index()
    //     .await
    //     .context("failed to delete search index")?;

    // delete_tenant_tables(&tenant_db)
    //     .await
    //     .context("failed to delete tenant tables")?;

    tenant
        .delete(&db)
        .await
        .context("failed to delete tenant")?;

    Ok(StatusCode::NO_CONTENT)
}

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
