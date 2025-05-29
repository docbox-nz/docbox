//! Document box related endpoints

use crate::{
    error::{DynHttpError, HttpCommonError, HttpErrorResponse, HttpResult, HttpStatusResult},
    middleware::{
        action_user::{ActionUser, UserParams},
        tenant::{TenantDb, TenantEvents, TenantParams, TenantSearch, TenantStorage},
    },
    models::document_box::{
        CreateDocumentBoxRequest, DocumentBoxResponse, DocumentBoxStats, HttpDocumentBoxError,
    },
};
use axum::{extract::Path, http::StatusCode, Json};
use axum_valid::Garde;
use docbox_core::{
    document_box::{
        create_document_box::{create_document_box, CreateDocumentBox, CreateDocumentBoxError},
        delete_document_box::{delete_document_box, DeleteDocumentBoxError},
        search_document_box::{search_document_box, ResolvedSearchResult},
    },
    search::models::{SearchRequest, SearchResultItem, SearchResultResponse},
};
use docbox_database::models::{
    document_box::{DocumentBox, DocumentBoxScope},
    folder::{self, Folder, FolderWithExtra, ResolvedFolderWithExtra},
};

pub const DOCUMENT_BOX_TAG: &str = "Document Box";

/// Create document box
///
/// Creates a new document box using the requested scope
#[utoipa::path(
    post,
    operation_id = "document_box_create",
    tag = DOCUMENT_BOX_TAG,
    path = "/box",
    responses(
        (status = 201, description = "Document box created successfully", body = DocumentBoxResponse),
        (status = 409, description = "Scope already exists", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(
        TenantParams,
        UserParams
    )
)]
#[tracing::instrument(skip_all, fields(req = ?req))]
pub async fn create(
    action_user: ActionUser,
    TenantDb(db): TenantDb,
    TenantEvents(events): TenantEvents,
    Garde(Json(req)): Garde<Json<CreateDocumentBoxRequest>>,
) -> Result<(StatusCode, Json<DocumentBoxResponse>), DynHttpError> {
    // Update stored editing user data
    let created_by = action_user.store_user(&db).await?;

    let create = CreateDocumentBox {
        scope: req.scope,
        created_by: created_by.as_ref().map(|value| value.id.to_string()),
    };

    let (document_box, root) =
        create_document_box(&db, &events, create)
            .await
            .map_err(|cause| match cause {
                CreateDocumentBoxError::ScopeAlreadyExists => {
                    DynHttpError::from(HttpDocumentBoxError::ScopeAlreadyExists)
                }
                cause => {
                    tracing::error!(?cause, "failed to create document box");
                    DynHttpError::from(HttpCommonError::ServerError)
                }
            })?;

    Ok((
        StatusCode::CREATED,
        Json(DocumentBoxResponse {
            document_box,
            root: FolderWithExtra {
                id: root.id,
                name: root.name,
                folder_id: root.folder_id,
                created_at: root.created_at,
                created_by: folder::CreatedByUser(created_by),
                last_modified_at: None,
                last_modified_by: folder::LastModifiedByUser(None),
            },
            children: Default::default(),
        }),
    ))
}

/// Get document box by scope
///
/// Gets a specific document box and the root folder for the box
/// along with the resolved root folder children
#[utoipa::path(
    get,
    operation_id = "document_box_get",
    tag = DOCUMENT_BOX_TAG,
    path = "/box/{scope}",
    responses(
        (status = 200, description = "Document box obtained successfully", body = DocumentBoxResponse),
        (status = 404, description = "Document box not found", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(
        ("scope" = String, Path, description = "Scope of the document box"),
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(scope = %scope))]
pub async fn get(
    TenantDb(db): TenantDb,
    Path(scope): Path<DocumentBoxScope>,
) -> HttpResult<DocumentBoxResponse> {
    let document_box = DocumentBox::find_by_scope(&db, &scope)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query document box");
            HttpCommonError::ServerError
        })?
        .ok_or(HttpDocumentBoxError::UnknownDocumentBox)?;

    let root = Folder::find_root_with_extra(&db, &scope)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query folder");
            HttpCommonError::ServerError
        })?
        .ok_or_else(|| {
            tracing::error!("document box missing root");
            HttpCommonError::ServerError
        })?;

    let children = ResolvedFolderWithExtra::resolve(&db, root.id)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query document box root folder");
            HttpCommonError::ServerError
        })?;

    Ok(Json(DocumentBoxResponse {
        document_box,
        root,
        children,
    }))
}

/// Get document box stats by scope
///
/// Requests stats about a document box using its scope. Provides stats such as:
/// - Total files
/// - Total links
/// - Total folders
#[utoipa::path(
    get,
    operation_id = "document_box_stats",
    tag = DOCUMENT_BOX_TAG,
    path = "/box/{scope}/stats",
    responses(
        (status = 200, description = "Document box stats obtained successfully", body = DocumentBoxStats),
        (status = 404, description = "Document box not found", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(
        ("scope" = String, Path, description = "Scope of the document box"),
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(scope = %scope))]
pub async fn stats(
    TenantDb(db): TenantDb,
    Path(scope): Path<DocumentBoxScope>,
) -> HttpResult<DocumentBoxStats> {
    // Assert that the document box exists
    let _document_box = DocumentBox::find_by_scope(&db, &scope)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query document box");
            HttpCommonError::ServerError
        })?
        .ok_or(HttpDocumentBoxError::UnknownDocumentBox)?;

    let root = Folder::find_root_with_extra(&db, &scope)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query folder");
            HttpCommonError::ServerError
        })?
        .ok_or_else(|| {
            tracing::error!("document box missing root");
            HttpCommonError::ServerError
        })?;

    let children = Folder::count_children(&db, root.id)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query document box children count");
            HttpCommonError::ServerError
        })?;

    Ok(Json(DocumentBoxStats {
        total_files: children.file_count,
        total_links: children.link_count,
        total_folders: children.folder_count,
    }))
}

/// Delete document box by scope
///
/// Deletes a specific document box by scope and all its contents
///
/// Access control for this should probably be restricted
/// on other end to prevent users from deleting an entire
/// bucket?
#[utoipa::path(
    delete,
    operation_id = "document_box_delete",
    tag = DOCUMENT_BOX_TAG,
    path = "/box/{scope}",
    responses(
        (status = 204, description = "Document box deleted successfully"),
        (status = 404, description = "Document box not found", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(
        ("scope" = String, Path, description = "Scope of the document box"),    
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(scope = %scope))]
pub async fn delete(
    TenantDb(db): TenantDb,
    TenantSearch(search): TenantSearch,
    TenantStorage(storage): TenantStorage,
    TenantEvents(events): TenantEvents,
    Path(scope): Path<DocumentBoxScope>,
) -> HttpStatusResult {
    delete_document_box(&db, &search, &storage, &events, scope)
        .await
        .map_err(|cause| match cause {
            DeleteDocumentBoxError::UnknownScope => {
                DynHttpError::from(HttpDocumentBoxError::UnknownDocumentBox)
            }

            cause => {
                tracing::error!(?cause, "failed to delete document box");
                DynHttpError::from(HttpCommonError::ServerError)
            }
        })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Search document box
///
/// Search within the document box
#[utoipa::path(
    post,
    operation_id = "document_box_search",
    tag = DOCUMENT_BOX_TAG,
    path = "/box/{scope}/search",
    responses(
        (status = 200, description = "Searched successfully", body = SearchResultResponse),
        (status = 400, description = "Malformed or invalid request not meeting validation requirements", body = HttpErrorResponse),
        (status = 404, description = "Target folder not found", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(
        ("scope" = String, Path, description = "Scope of the document box"),
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(scope = %scope, req = ?req))]
pub async fn search(
    TenantDb(db): TenantDb,
    TenantSearch(search): TenantSearch,
    Path(scope): Path<DocumentBoxScope>,
    Garde(Json(req)): Garde<Json<SearchRequest>>,
) -> HttpResult<SearchResultResponse> {
    let resolved = search_document_box(&db, &search, scope, req)
        .await
        .map_err(|error| {
            tracing::error!(?error, "failed to search document box");
            HttpCommonError::ServerError
        })?;

    let out: Vec<SearchResultItem> = resolved
        .results
        .into_iter()
        .map(
            |ResolvedSearchResult { result, data, path }| SearchResultItem {
                path,
                score: result.score,
                data,
                page_matches: result.page_matches,
                total_hits: result.total_hits,
                name_match: result.name_match,
                content_match: result.content_match,
            },
        )
        .collect();

    Ok(Json(SearchResultResponse {
        total_hits: resolved.total_hits,
        results: out,
    }))
}
