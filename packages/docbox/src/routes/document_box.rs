//! Document box related endpoints

use std::ops::DerefMut;

use crate::{
    error::{DynHttpError, HttpResult, HttpStatusResult},
    middleware::{
        action_user::ActionUser,
        tenant::{TenantDb, TenantEvents, TenantStorage, TenantSearch},
    },
    models::document_box::{
        CreateDocumentBox, DocumentBoxError, DocumentBoxOptions, DocumentBoxResponse,
        DocumentBoxStats,
    },
    MAX_FILE_SIZE,
};
use anyhow::Context;
use axum::{extract::Path, http::StatusCode, Json};
use axum_valid::Garde;
use docbox_core::{
    events::TenantEventMessage,
    search::{
        models::{
            FlattenedItemResult, SearchRequest, SearchResultData, SearchResultItem,
            SearchResultResponse,
        },
        os::resolve_search_result,
    },
    services::folders::delete_folder,
    utils::validation::ALLOWED_MIME_TYPES,
};
use docbox_database::models::{
    document_box::{DocumentBox, DocumentBoxScope},
    folder::{
        self, CreateFolder, Folder, FolderPathSegment, FolderWithExtra, ResolvedFolderWithExtra,
    },
};
use tracing::{debug, error};

/// POST /box
///
/// Creates a new tenant within the doc-box and creates the tables
/// for the tenant
pub async fn create(
    action_user: ActionUser,
    TenantDb(db): TenantDb,
    TenantEvents(events): TenantEvents,
    Garde(Json(req)): Garde<Json<CreateDocumentBox>>,
) -> Result<(StatusCode, Json<DocumentBoxResponse>), DynHttpError> {
    // Update stored editing user data
    let created_by = action_user.store_user(&db).await?;

    // Enter a database transaction
    let mut transaction = db.begin().await.context("failed to create transaction")?;

    // Create the document box
    let document_box: DocumentBox = DocumentBox::create(transaction.deref_mut(), req.scope.clone())
        .await
        .map_err(|err| {
            if let Some(db_err) = err.as_database_error() {
                // Handle attempts at a duplicate scope creation
                if db_err.is_unique_violation() {
                    return DynHttpError::from(DocumentBoxError::ScopeAlreadyExists);
                }
            }

            DynHttpError::from(err)
        })?;

    // Create the root folder
    let root: Folder = Folder::create(
        transaction.deref_mut(),
        CreateFolder {
            name: "Root".to_string(),
            document_box: document_box.scope.clone(),
            folder_id: None,
            created_by: created_by.as_ref().map(|value| value.id.to_string()),
        },
    )
    .await
    .context("failed to create root folder")?;

    transaction
        .commit()
        .await
        .context("failed to commit transaction")?;

    // Publish an event
    events.publish_event(TenantEventMessage::DocumentBoxCreated(document_box.clone()));

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

/// GET /options
///
/// Requests the document box options and settings
pub async fn get_options() -> HttpResult<DocumentBoxOptions> {
    Ok(Json(DocumentBoxOptions {
        allowed_mime_types: ALLOWED_MIME_TYPES,
        max_file_size: MAX_FILE_SIZE,
    }))
}

/// GET /box/:scope
///
/// Gets a specific document box and the root folder for the box
/// along with the resolved root folder children
pub async fn get(
    TenantDb(db): TenantDb,
    Path(scope): Path<DocumentBoxScope>,
) -> HttpResult<DocumentBoxResponse> {
    let document_box = DocumentBox::find_by_scope(&db, &scope)
        .await?
        .ok_or(DocumentBoxError::UnknownDocumentBox)?;

    let root = Folder::find_root_with_extra(&db, &scope)
        .await?
        .ok_or(DocumentBoxError::MissingDocumentBoxRoot)?;

    let children = ResolvedFolderWithExtra::resolve(&db, root.id).await?;

    Ok(Json(DocumentBoxResponse {
        document_box,
        root,
        children,
    }))
}

/// GET /box/:scope/stats
///
/// Gets a specific document box and the root folder for the box
/// along with the resolved root folder children
pub async fn stats(
    TenantDb(db): TenantDb,
    Path(scope): Path<DocumentBoxScope>,
) -> HttpResult<DocumentBoxStats> {
    let _document_box = DocumentBox::find_by_scope(&db, &scope)
        .await?
        .ok_or(DocumentBoxError::UnknownDocumentBox)?;

    let root = Folder::find_root_with_extra(&db, &scope)
        .await?
        .ok_or(DocumentBoxError::MissingDocumentBoxRoot)?;

    let children = Folder::count_children(&db, root.id).await?;

    debug!(?children, "loaded folder children stats");

    Ok(Json(DocumentBoxStats {
        total_files: children.file_count,
        total_links: children.link_count,
        total_folders: children.folder_count,
    }))
}

/// DELETE /box/:scope
///
/// Deletes a specific document box and all its contents
///
/// Access control for this should probably be restricted
/// on other end to prevent users from deleting an entire
/// bucket?
pub async fn delete(
    TenantDb(db): TenantDb,
    TenantSearch(opensearch): TenantSearch,
    TenantStorage(s3): TenantStorage,
    TenantEvents(events): TenantEvents,
    Path(scope): Path<DocumentBoxScope>,
) -> HttpStatusResult {
    let document_box = DocumentBox::find_by_scope(&db, &scope)
        .await?
        .ok_or(DocumentBoxError::UnknownDocumentBox)?;

    let root = Folder::find_root(&db, &scope).await?;

    if let Some(root) = root {
        // Delete root folder
        if let Err(cause) = delete_folder(&db, &s3, &opensearch, &events, root).await {
            tracing::error!(?cause, "failed to delete bucket root folder");
            return Err(anyhow::Error::msg("failed to delete bucket root folder").into());
        };
    }

    // Delete document box
    if let Err(cause) = document_box.delete(&db).await {
        tracing::error!(?cause, "failed to delete document box");
        return Err(anyhow::Error::msg("failed to delete document box").into());
    }

    opensearch.delete_by_scope(scope).await?;

    // Publish an event
    events.publish_event(TenantEventMessage::DocumentBoxDeleted(document_box));

    Ok(StatusCode::NO_CONTENT)
}

/// POST /box/:scope/search
///
/// Search within the document box
pub async fn search(
    TenantDb(db): TenantDb,
    TenantSearch(opensearch): TenantSearch,
    Path(scope): Path<DocumentBoxScope>,
    Garde(Json(req)): Garde<Json<SearchRequest>>,
) -> HttpResult<SearchResultResponse> {
    let search_folder_ids = match req.folder_id {
        Some(folder_id) => {
            let folder = Folder::find_by_id(&db, &scope, folder_id)
                .await
                .context("failed to query search folder")?
                .context("unable to find search folder")?;

            let folder_children = folder
                .tree_all_children(&db)
                .await
                .context("failed to query search folder children")?;

            Some(folder_children)
        }
        None => None,
    };

    debug!(?search_folder_ids, "searching within folders");

    let results = opensearch
        .search_index(&[scope], req, search_folder_ids)
        .await?;

    let mut resolved: Vec<(
        FlattenedItemResult,
        SearchResultData,
        Vec<FolderPathSegment>,
    )> = Vec::with_capacity(results.results.len());

    for result in results.results {
        match resolve_search_result(&db, result).await {
            Ok(value) => resolved.push(value),
            Err(cause) => {
                error!("failed to fetch search result from database {cause:?}");
                continue;
            }
        }
    }

    let out: Vec<SearchResultItem> = resolved
        .into_iter()
        .map(|(hit, data, path)| SearchResultItem {
            path,
            score: hit.score,
            data,
            page_matches: hit.page_matches,
            total_hits: hit.total_hits,
            name_match: hit.name_match,
            content_match: hit.content_match,
        })
        .collect();

    Ok(Json(SearchResultResponse {
        total_hits: results.total_hits,
        results: out,
    }))
}
