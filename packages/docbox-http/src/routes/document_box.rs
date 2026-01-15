//! Document box related endpoints

use crate::{
    error::{DynHttpError, HttpCommonError, HttpErrorResponse, HttpResult, HttpStatusResult},
    middleware::{
        action_user::{ActionUser, UserParams},
        tenant::{TenantDb, TenantEvents, TenantParams, TenantSearch, TenantStorage},
    },
    models::document_box::{
        CreateDocumentBoxRequest, DocumentBoxResponse, DocumentBoxScope, DocumentBoxStats,
        HttpDocumentBoxError,
    },
};
use axum::{Json, extract::Path, http::StatusCode};
use axum_valid::Garde;
use docbox_core::{
    database::models::{
        document_box::DocumentBox,
        file::File,
        folder::{Folder, FolderWithExtra, ResolvedFolderWithExtra},
        shared::WithFullPath,
    },
    document_box::{
        create_document_box::{CreateDocumentBox, CreateDocumentBoxError, create_document_box},
        delete_document_box::{DeleteDocumentBoxError, delete_document_box},
        search_document_box::{ResolvedSearchResult, search_document_box},
    },
    search::models::{SearchRequest, SearchResultItem, SearchResultResponse},
};
use tokio::join;

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
#[tracing::instrument(skip_all, fields(?req))]
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
            .map_err(|error| match error {
                CreateDocumentBoxError::ScopeAlreadyExists => {
                    DynHttpError::from(HttpDocumentBoxError::ScopeAlreadyExists)
                }
                error => {
                    tracing::error!(?error, "failed to create document box");
                    DynHttpError::from(HttpCommonError::ServerError)
                }
            })?;

    Ok((
        StatusCode::CREATED,
        Json(DocumentBoxResponse {
            document_box,
            root: FolderWithExtra {
                folder: root,
                created_by,
                last_modified_at: None,
                last_modified_by: None,
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
        ("scope" = DocumentBoxScope, Path, description = "Scope of the document box"),
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(%scope))]
pub async fn get(
    TenantDb(db): TenantDb,
    Path(DocumentBoxScope(scope)): Path<DocumentBoxScope>,
) -> HttpResult<DocumentBoxResponse> {
    let document_box = DocumentBox::find_by_scope(&db, &scope)
        .await
        .map_err(|error| {
            tracing::error!(?error, "failed to query document box");
            HttpCommonError::ServerError
        })?
        .ok_or(HttpDocumentBoxError::UnknownDocumentBox)?;

    let WithFullPath {
        data: root,
        full_path,
    } = Folder::find_root_with_extra(&db, &scope)
        .await
        .map_err(|error| {
            tracing::error!(?error, "failed to query folder");
            HttpCommonError::ServerError
        })?
        .ok_or_else(|| {
            tracing::error!("document box missing root");
            HttpCommonError::ServerError
        })?;

    let children = ResolvedFolderWithExtra::resolve(&db, root.folder.id, full_path)
        .await
        .map_err(|error| {
            tracing::error!(?error, "failed to query document box root folder");
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
/// - Size of all files
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
        ("scope" = DocumentBoxScope, Path, description = "Scope of the document box"),
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(%scope))]
pub async fn stats(
    TenantDb(db): TenantDb,
    Path(DocumentBoxScope(scope)): Path<DocumentBoxScope>,
) -> HttpResult<DocumentBoxStats> {
    // Assert that the document box exists
    let _document_box = DocumentBox::find_by_scope(&db, &scope)
        .await
        .map_err(|error| {
            tracing::error!(?error, "failed to query document box");
            HttpCommonError::ServerError
        })?
        .ok_or(HttpDocumentBoxError::UnknownDocumentBox)?;

    let root = Folder::find_root(&db, &scope)
        .await
        .map_err(|error| {
            tracing::error!(?error, "failed to query folder");
            HttpCommonError::ServerError
        })?
        .ok_or_else(|| {
            tracing::error!("document box missing root");
            HttpCommonError::ServerError
        })?;

    let children_future = Folder::count_children(&db, root.id);
    let file_size_future = File::total_size_within_scope(&db, &scope);

    // Load the children count and file sizes in parallel
    let (children, file_size) = join!(children_future, file_size_future);

    let children = children.map_err(|error| {
        tracing::error!(?error, "failed to query document box children count");
        HttpCommonError::ServerError
    })?;

    let file_size = file_size.map_err(|error| {
        tracing::error!(?error, "failed to query document box file size");
        HttpCommonError::ServerError
    })?;

    Ok(Json(DocumentBoxStats {
        total_files: children.file_count,
        total_links: children.link_count,
        total_folders: children.folder_count,
        file_size,
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
        ("scope" = DocumentBoxScope, Path, description = "Scope of the document box"),
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(%scope))]
pub async fn delete(
    TenantDb(db): TenantDb,
    TenantSearch(search): TenantSearch,
    TenantStorage(storage): TenantStorage,
    TenantEvents(events): TenantEvents,
    Path(DocumentBoxScope(scope)): Path<DocumentBoxScope>,
) -> HttpStatusResult {
    delete_document_box(&db, &search, &storage, &events, &scope)
        .await
        .map_err(|error| match error {
            DeleteDocumentBoxError::UnknownScope => HttpDocumentBoxError::UnknownDocumentBox.into(),
            error => {
                tracing::error!(?error, "failed to delete document box");
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
        ("scope" = DocumentBoxScope, Path, description = "Scope of the document box"),
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(%scope, ?req))]
pub async fn search(
    TenantDb(db): TenantDb,
    TenantSearch(search): TenantSearch,
    Path(DocumentBoxScope(scope)): Path<DocumentBoxScope>,
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
