//! Folder related endpoints

use axum::{extract::Path, http::StatusCode, Json};
use axum_valid::Garde;
use std::ops::DerefMut;

use crate::{
    error::{DynHttpError, HttpCommonError, HttpErrorResponse, HttpResult, HttpStatusResult},
    middleware::{
        action_user::{ActionUser, UserParams},
        tenant::{TenantDb, TenantEvents, TenantParams, TenantSearch, TenantStorage},
    },
    models::folder::{CreateFolderRequest, FolderResponse, HttpFolderError, UpdateFolderRequest},
};
use docbox_core::{
    search::models::UpdateSearchIndexData,
    services::folders::{
        delete_folder, move_folder, safe_create_folder, update_folder_name, CreateFolderData,
    },
};
use docbox_database::models::{
    document_box::DocumentBoxScope,
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
        ("scope" = String, Path, description = "Scope to create the folder within"),
        TenantParams,
        UserParams
    )
)]
#[tracing::instrument(skip_all, fields(scope, req))]
pub async fn create(
    action_user: ActionUser,
    TenantDb(db): TenantDb,
    TenantSearch(search): TenantSearch,
    TenantEvents(events): TenantEvents,
    Path(scope): Path<DocumentBoxScope>,
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
        ("scope" = String, Path, description = "Scope the folder resides within"),
        ("folder_id" = Uuid, Path, description = "ID of the folder to request"),
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(scope, folder_id))]
pub async fn get(
    TenantDb(db): TenantDb,
    Path((scope, folder_id)): Path<(DocumentBoxScope, FolderId)>,
) -> HttpResult<FolderResponse> {
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
        ("scope" = String, Path, description = "Scope the folder resides within"),
        ("folder_id" = Uuid, Path, description = "ID of the folder to request"),
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(scope, folder_id))]
pub async fn get_edit_history(
    TenantDb(db): TenantDb,
    Path((scope, folder_id)): Path<(DocumentBoxScope, FolderId)>,
) -> HttpResult<Vec<EditHistory>> {
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
        ("scope" = String, Path, description = "Scope the folder resides within"),
        ("folder_id" = Uuid, Path, description = "ID of the folder to request"),
        TenantParams,
        UserParams
    )
)]
#[tracing::instrument(skip_all, fields(scope, folder_id, req))]
pub async fn update(
    action_user: ActionUser,
    TenantDb(db): TenantDb,
    TenantSearch(opensearch): TenantSearch,
    Path((scope, folder_id)): Path<(DocumentBoxScope, FolderId)>,
    Garde(Json(req)): Garde<Json<UpdateFolderRequest>>,
) -> HttpStatusResult {
    let mut folder = Folder::find_by_id(&db, &scope, folder_id)
        .await
        // Failed to query folder
        .map_err(|cause| {
            tracing::error!(?scope, ?folder_id, ?cause, "failed to query folder");
            HttpCommonError::ServerError
        })?
        // Folder not found
        .ok_or(HttpFolderError::UnknownFolder)?;

    // Cannot modify the root folder, this is not allowed
    if folder.folder_id.is_none() {
        return Err(HttpFolderError::CannotModifyRoot.into());
    }

    let mut db = db.begin().await.map_err(|cause| {
        tracing::error!(?cause, "failed to begin transaction");
        HttpCommonError::ServerError
    })?;

    // Update stored editing user data
    let user = action_user.store_user(db.deref_mut()).await?;
    let user_id = user.as_ref().map(|value| value.id.to_string());

    if let Some(target_id) = req.folder_id {
        // Cannot move folder into itself
        if target_id == folder.id {
            return Err(HttpFolderError::CannotMoveIntoSelf.into());
        }

        // Ensure the target folder exists, also ensures the target folder is in the same scope
        // (We may allow across scopes in the future, but would need additional checks for access control of target scope)
        let target_folder = Folder::find_by_id(db.deref_mut(), &scope, target_id)
            .await
            .map_err(|cause| {
                tracing::error!(?cause, "failed to query folder");
                HttpCommonError::ServerError
            })?
            .ok_or(HttpFolderError::UnknownTargetFolder)?;

        folder = move_folder(&mut db, user_id.clone(), folder, target_folder)
            .await
            .map_err(|cause| {
                tracing::error!(?cause, "failed to move folder");
                HttpCommonError::ServerError
            })?;
    };

    if let Some(new_name) = req.name {
        folder = update_folder_name(&mut db, user_id, folder, new_name)
            .await
            .map_err(|cause| {
                tracing::error!(?cause, "failed to update folder name");
                HttpCommonError::ServerError
            })?;
    }

    // Update search index data
    opensearch
        .update_data(
            folder.id,
            UpdateSearchIndexData {
                folder_id: folder.folder_id,
                name: Some(folder.name.clone()),
                mime: None,
                content: None,
                created_at: None,
                created_by: None,
                document_box: None,
                pages: None,
            },
        )
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to update search index data");
            HttpCommonError::ServerError
        })?;

    db.commit().await.map_err(|cause| {
        tracing::error!(?cause, "failed to commit transaction");
        HttpCommonError::ServerError
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
        ("scope" = String, Path, description = "Scope the folder resides within"),
        ("folder_id" = Uuid, Path, description = "ID of the folder to delete"),
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(scope, folder_id))]
pub async fn delete(
    TenantDb(db): TenantDb,
    TenantStorage(s3): TenantStorage,
    TenantEvents(events): TenantEvents,
    TenantSearch(opensearch): TenantSearch,
    Path((scope, folder_id)): Path<(DocumentBoxScope, FolderId)>,
) -> HttpStatusResult {
    let folder = Folder::find_by_id(&db, &scope, folder_id)
        .await
        // Failed to query folder
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query folder");
            HttpCommonError::ServerError
        })?
        // Folder not found
        .ok_or(HttpFolderError::UnknownFolder)?;

    delete_folder(&db, &s3, &opensearch, &events, folder)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to delete folder");
            HttpCommonError::ServerError
        })?;

    Ok(StatusCode::NO_CONTENT)
}
