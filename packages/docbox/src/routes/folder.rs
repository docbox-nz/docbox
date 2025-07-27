//! Folder related endpoints

use crate::{
    error::{DynHttpError, HttpCommonError, HttpErrorResponse, HttpResult, HttpStatusResult},
    middleware::{
        action_user::{ActionUser, UserParams},
        tenant::{TenantDb, TenantEvents, TenantParams, TenantSearch, TenantStorage},
    },
    models::{
        document_box::DocumentBoxScope,
        folder::{CreateFolderRequest, FolderResponse, HttpFolderError, UpdateFolderRequest},
    },
};
use axum::{Json, extract::Path, http::StatusCode};
use axum_valid::Garde;
use docbox_core::folders::{
    create_folder::{CreateFolderData, safe_create_folder},
    delete_folder::delete_folder,
    update_folder::{UpdateFolder, UpdateFolderError},
};
use docbox_database::models::{
    edit_history::EditHistory,
    folder::{self, Folder, FolderId, FolderWithExtra, ResolvedFolderWithExtra},
};

pub const FOLDER_TAG: &str = "Folder";

/// Create folder
///
/// Creates a new folder in the provided document box folder
#[utoipa::path(
    post,
    operation_id = "folder_create",
    tag = FOLDER_TAG,
    path = "/box/{scope}/folder",
    responses(
        (status = 201, description = "Folder created successfully", body = FolderResponse),
        (status = 404, description = "Destination folder not found", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(
        ("scope" = DocumentBoxScope, Path, description = "Scope to create the folder within"),
        TenantParams,
        UserParams
    )
)]
#[tracing::instrument(skip_all, fields(scope = %scope, req = ?req))]
pub async fn create(
    action_user: ActionUser,
    TenantDb(db): TenantDb,
    TenantSearch(search): TenantSearch,
    TenantEvents(events): TenantEvents,
    Path(DocumentBoxScope(scope)): Path<DocumentBoxScope>,
    Garde(Json(req)): Garde<Json<CreateFolderRequest>>,
) -> Result<(StatusCode, Json<FolderResponse>), DynHttpError> {
    let folder_id = req.folder_id;
    let parent_folder = Folder::find_by_id(&db, &scope, folder_id)
        .await
        // Failed to query destination folder
        .map_err(|cause| {
            tracing::error!(
                ?scope,
                ?folder_id,
                ?cause,
                "failed to query link destination folder"
            );
            HttpCommonError::ServerError
        })?
        // Folder not found
        .ok_or(HttpFolderError::UnknownFolder)?;

    // Update stored editing user data
    let created_by = action_user.store_user(&db).await?;

    // Make the create query
    let create = CreateFolderData {
        folder: parent_folder,
        name: req.name,
        created_by: created_by.as_ref().map(|value| value.id.to_string()),
    };

    // Perform Folder creation
    let folder = safe_create_folder(&db, search, &events, create)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to create link");
            HttpFolderError::CreateError(cause)
        })?;

    Ok((
        StatusCode::CREATED,
        Json(FolderResponse {
            folder: FolderWithExtra {
                id: folder.id,
                name: folder.name,
                folder_id: folder.folder_id,
                created_at: folder.created_at,
                created_by: folder::CreatedByUser(created_by),
                last_modified_at: None,
                last_modified_by: folder::LastModifiedByUser(None),
                pinned: folder.pinned,
            },
            children: ResolvedFolderWithExtra::default(),
        }),
    ))
}

/// Get folder by ID
///
/// Requests a specific folder by ID. Will return the folder itself
/// as well as the first resolved set of children for the folder
#[utoipa::path(
    get,
    operation_id = "folder_get",
    tag = FOLDER_TAG,
    path = "/box/{scope}/folder/{folder_id}",
    responses(
        (status = 200, description = "Folder obtained successfully", body = FolderResponse),
        (status = 404, description = "Folder not found", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(
        ("scope" = DocumentBoxScope, Path, description = "Scope the folder resides within"),
        ("folder_id" = Uuid, Path, description = "ID of the folder to request"),
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(scope = %scope, folder_id = %folder_id))]
pub async fn get(
    TenantDb(db): TenantDb,
    Path((scope, folder_id)): Path<(DocumentBoxScope, FolderId)>,
) -> HttpResult<FolderResponse> {
    let DocumentBoxScope(scope) = scope;

    let folder = Folder::find_by_id_with_extra(&db, &scope, folder_id)
        .await
        // Failed to query folder
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query folder");
            HttpCommonError::ServerError
        })?
        // Folder not found
        .ok_or(HttpFolderError::UnknownFolder)?;

    let children = ResolvedFolderWithExtra::resolve(&db, folder.id)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to resolve folder children");
            HttpCommonError::ServerError
        })?;
    Ok(Json(FolderResponse { folder, children }))
}

/// Get folder edit history
///
/// Request the edit history for the provided folder
#[utoipa::path(
    get,
    operation_id = "folder_edit_history",
    tag = FOLDER_TAG,
    path = "/box/{scope}/folder/{folder_id}/edit-history",
    responses(
        (status = 200, description = "Obtained edit history", body = [EditHistory]),
        (status = 404, description = "Folder not found", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(
        ("scope" = DocumentBoxScope, Path, description = "Scope the folder resides within"),
        ("folder_id" = Uuid, Path, description = "ID of the folder to request"),
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(scope = %scope, folder_id = %folder_id))]
pub async fn get_edit_history(
    TenantDb(db): TenantDb,
    Path((scope, folder_id)): Path<(DocumentBoxScope, FolderId)>,
) -> HttpResult<Vec<EditHistory>> {
    let DocumentBoxScope(scope) = scope;

    _ = Folder::find_by_id_with_extra(&db, &scope, folder_id)
        .await
        // Failed to query folder
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query folder");
            HttpCommonError::ServerError
        })?
        // Folder not found
        .ok_or(HttpFolderError::UnknownFolder)?;

    let edit_history = EditHistory::all_by_folder(&db, folder_id)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query folder edit history");
            HttpCommonError::ServerError
        })?;

    Ok(Json(edit_history))
}

/// Update folder
///
/// Updates a folder, can be a name change, a folder move, or both
#[utoipa::path(
    put,
    operation_id = "folder_update",
    tag = FOLDER_TAG,
    path = "/box/{scope}/folder/{folder_id}",
    responses(
        (status = 200, description = "Updated folder successfully"),
        (status = 400, description = "Attempted to move a root folder or a folder into itself", body = HttpErrorResponse),
        (status = 404, description = "Folder not found", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(
        ("scope" = DocumentBoxScope, Path, description = "Scope the folder resides within"),
        ("folder_id" = Uuid, Path, description = "ID of the folder to request"),
        TenantParams,
        UserParams
    )
)]
#[tracing::instrument(skip_all, fields(scope = %scope, folder_id = %folder_id, req = ?req))]
pub async fn update(
    action_user: ActionUser,
    TenantDb(db): TenantDb,
    TenantSearch(search): TenantSearch,
    Path((scope, folder_id)): Path<(DocumentBoxScope, FolderId)>,
    Garde(Json(req)): Garde<Json<UpdateFolderRequest>>,
) -> HttpStatusResult {
    let DocumentBoxScope(scope) = scope;

    let folder = Folder::find_by_id(&db, &scope, folder_id)
        .await
        // Failed to query folder
        .map_err(|cause| {
            tracing::error!(?scope, ?folder_id, ?cause, "failed to query folder");
            HttpCommonError::ServerError
        })?
        // Folder not found
        .ok_or(HttpFolderError::UnknownFolder)?;

    // Update stored editing user data
    let user = action_user.store_user(&db).await?;
    let user_id = user.as_ref().map(|value| value.id.to_string());

    let update = UpdateFolder {
        folder_id: req.folder_id,
        name: req.name,
        pinned: req.pinned,
    };

    docbox_core::folders::update_folder::update_folder(
        &db, &search, &scope, folder, user_id, update,
    )
    .await
    .map_err(|err| match err {
        UpdateFolderError::UnknownTargetFolder => {
            DynHttpError::from(HttpFolderError::UnknownTargetFolder)
        }
        UpdateFolderError::CannotModifyRoot => {
            DynHttpError::from(HttpFolderError::CannotModifyRoot)
        }
        UpdateFolderError::CannotMoveIntoSelf => {
            DynHttpError::from(HttpFolderError::CannotMoveIntoSelf)
        }
        _ => DynHttpError::from(HttpCommonError::ServerError),
    })?;

    Ok(StatusCode::OK)
}

/// Delete a folder by ID
///
/// Deletes a document box folder and all its contents. This will
/// traverse the folder contents as a stack deleting all files and
/// folders within the folder before deleting itself
#[utoipa::path(
    delete,
    operation_id = "folder_delete",
    tag = FOLDER_TAG,
    path = "/box/{scope}/folder/{folder_id}",
    responses(
        (status = 204, description = "Deleted folder successfully"),
        (status = 404, description = "Folder not found", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(
        ("scope" = DocumentBoxScope, Path, description = "Scope the folder resides within"),
        ("folder_id" = Uuid, Path, description = "ID of the folder to delete"),
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(scope = %scope, folder_id = %folder_id))]
pub async fn delete(
    TenantDb(db): TenantDb,
    TenantStorage(storage): TenantStorage,
    TenantEvents(events): TenantEvents,
    TenantSearch(search): TenantSearch,
    Path((scope, folder_id)): Path<(DocumentBoxScope, FolderId)>,
) -> HttpStatusResult {
    let DocumentBoxScope(scope) = scope;

    let folder = Folder::find_by_id(&db, &scope, folder_id)
        .await
        // Failed to query folder
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query folder");
            HttpCommonError::ServerError
        })?
        // Folder not found
        .ok_or(HttpFolderError::UnknownFolder)?;

    // Root folder cannot be deleted through the API
    if folder.folder_id.is_none() {
        return Err(HttpFolderError::CannotDeleteRoot.into());
    }

    delete_folder(&db, &storage, &search, &events, folder)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to delete folder");
            HttpCommonError::ServerError
        })?;

    Ok(StatusCode::NO_CONTENT)
}
