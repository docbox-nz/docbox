//! Folder related endpoints

use std::ops::DerefMut;

use anyhow::Context;
use axum::{extract::Path, http::StatusCode, Json};
use axum_valid::Garde;

use crate::{
    error::{DynHttpError, HttpErrorResponse, HttpResult, HttpStatusResult},
    middleware::{
        action_user::ActionUser,
        tenant::{TenantDb, TenantEvents, TenantSearch, TenantStorage},
    },
    models::folder::{CreateFolderRequest, FolderResponse, HttpFolderError, UpdateFolderRequest},
};
use docbox_core::{
    search::models::UpdateSearchIndexData,
    services::folders::{
        create_folder, delete_folder, move_folder, rollback_create_folder, update_folder_name,
        CreateFolderData, CreateFolderState,
    },
};
use docbox_database::models::{
    document_box::DocumentBoxScope,
    edit_history::EditHistory,
    folder::{self, Folder, FolderId, FolderWithExtra, ResolvedFolderWithExtra},
};

pub const FOLDER_TAG: &str = "folder";

/// Create folder
///
/// Creates a new folder in the provided document box folder
#[utoipa::path(
    post,
    tag = FOLDER_TAG,
    path = "/box/{scope}/folder",
    responses(
        (status = 201, description = "Folder created successfully", body = FolderResponse),
        (status = 404, description = "Destination folder not found", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(
        ("scope" = String, Path, description = "Scope to create the folder within"),
    )
)]
pub async fn create(
    action_user: ActionUser,
    TenantDb(db): TenantDb,
    TenantSearch(opensearch): TenantSearch,
    TenantEvents(events): TenantEvents,
    Path(scope): Path<DocumentBoxScope>,
    Garde(Json(req)): Garde<Json<CreateFolderRequest>>,
) -> Result<(StatusCode, Json<FolderResponse>), DynHttpError> {
    let parent_folder = Folder::find_by_id(&db, &scope, req.folder_id)
        .await
        .context("unable to query folder")?
        .ok_or(HttpFolderError::UnknownFolder)?;

    // Update stored editing user data
    let created_by = action_user.store_user(&db).await?;

    let mut create_state = CreateFolderState::default();

    let create = CreateFolderData {
        folder: parent_folder,
        name: req.name,
        created_by: created_by.as_ref().map(|value| value.id.to_string()),
    };

    let folder = match create_folder(&db, &opensearch, &events, create, &mut create_state).await {
        Ok(value) => value,
        Err(err) => {
            // Attempt to rollback any allocated resources in the background
            tokio::spawn(async move {
                rollback_create_folder(&opensearch, create_state).await;
            });

            return Err(anyhow::Error::from(err).into());
        }
    };

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

/// GET /box/:scope/folder/:folder_id
///
/// Gets a specific folder within a document box, resolves the folder
/// children including them in the response
pub async fn get(
    TenantDb(db): TenantDb,
    Path((scope, folder_id)): Path<(DocumentBoxScope, FolderId)>,
) -> HttpResult<FolderResponse> {
    let folder = Folder::find_by_id_with_extra(&db, &scope, folder_id)
        .await?
        .ok_or(HttpFolderError::UnknownFolder)?;

    let children = ResolvedFolderWithExtra::resolve(&db, folder.id).await?;

    Ok(Json(FolderResponse { folder, children }))
}
/// GET /box/:scope/folder/:folder_id/edit-history
///
/// Gets the edit history for a specific folder
pub async fn get_edit_history(
    TenantDb(db): TenantDb,
    Path((scope, folder_id)): Path<(DocumentBoxScope, FolderId)>,
) -> HttpResult<Vec<EditHistory>> {
    _ = Folder::find_by_id_with_extra(&db, &scope, folder_id)
        .await?
        .ok_or(HttpFolderError::UnknownFolder)?;

    let edit_history = EditHistory::all_by_folder(&db, folder_id)
        .await
        .context("failed to get edit history")?;

    Ok(Json(edit_history))
}

/// PUT /box/:scope/folder/:folder_id
///
/// Updates a folder, can be a name change, a folder move, or both
pub async fn update(
    action_user: ActionUser,
    TenantDb(db): TenantDb,
    TenantSearch(opensearch): TenantSearch,
    Path((scope, folder_id)): Path<(DocumentBoxScope, FolderId)>,
    Garde(Json(req)): Garde<Json<UpdateFolderRequest>>,
) -> HttpStatusResult {
    let mut folder = Folder::find_by_id(&db, &scope, folder_id)
        .await
        .context("failed to query file")?
        .ok_or(HttpFolderError::UnknownFolder)?;

    // Cannot modify the root folder, this is not allowed
    if folder.folder_id.is_none() {
        return Err(HttpFolderError::CannotModifyRoot.into());
    }

    let mut db = db.begin().await.context("failed to start transaction")?;

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
            .context("unknown target folder")?
            .ok_or(HttpFolderError::UnknownTargetFolder)?;

        folder = move_folder(&mut db, user_id.clone(), folder, target_folder).await?;
    };

    if let Some(new_name) = req.name {
        folder = update_folder_name(&mut db, user_id, folder, new_name).await?;
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
        .context("failed to update search index for new file name")?;

    db.commit().await.context("failed to commit transaction")?;

    Ok(StatusCode::OK)
}

/// DELETE /box/:scope/folder/:folder_id
///
/// Deletes a document box folder and all its contents
pub async fn delete(
    TenantDb(db): TenantDb,
    TenantStorage(s3): TenantStorage,
    TenantEvents(events): TenantEvents,
    TenantSearch(opensearch): TenantSearch,
    Path((scope, folder_id)): Path<(DocumentBoxScope, FolderId)>,
) -> HttpStatusResult {
    let folder = Folder::find_by_id(&db, &scope, folder_id)
        .await
        .context("failed to query file")?
        .ok_or(HttpFolderError::UnknownFolder)?;

    delete_folder(&db, &s3, &opensearch, &events, folder)
        .await
        .context("failed to delete folder")?;

    Ok(StatusCode::NO_CONTENT)
}
