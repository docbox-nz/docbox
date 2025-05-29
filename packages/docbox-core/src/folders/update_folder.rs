use docbox_database::{
    models::{
        document_box::DocumentBoxScope,
        edit_history::{
            CreateEditHistory, CreateEditHistoryType, EditHistory, EditHistoryMetadata,
        },
        folder::{Folder, FolderId},
        user::UserId,
    },
    DbErr, DbPool, DbResult, DbTransaction,
};
use std::ops::DerefMut;
use thiserror::Error;

use crate::search::{models::UpdateSearchIndexData, TenantSearchIndex};

#[derive(Debug, Error)]
pub enum UpdateFolderError {
    /// Database related error
    #[error(transparent)]
    Database(#[from] DbErr),

    /// Target folder could not be found
    #[error("unknown target folder")]
    UnknownTargetFolder,

    /// Modification of the root folder is not allowed
    #[error("cannot modify root")]
    CannotModifyRoot,

    /// Attempted to move a folder into itself
    #[error("cannot move into self")]
    CannotMoveIntoSelf,

    /// Failed to update the search index
    #[error(transparent)]
    SearchIndex(anyhow::Error),
}

pub struct UpdateFolder {
    /// Move the folder to another folder
    pub folder_id: Option<FolderId>,

    /// Update the folder name
    pub name: Option<String>,
}

pub async fn update_folder(
    db: &DbPool,
    search: &TenantSearchIndex,
    scope: &DocumentBoxScope,
    folder: Folder,
    user_id: Option<String>,
    update: UpdateFolder,
) -> Result<(), UpdateFolderError> {
    let mut folder = folder;

    let folder_id = folder
        .folder_id
        // Cannot modify the root folder, this is not allowed
        .ok_or(UpdateFolderError::CannotModifyRoot)?;

    let mut db = db
        .begin()
        .await
        .inspect_err(|cause| tracing::error!(?cause, "failed to begin transaction"))?;

    if let Some(target_id) = update.folder_id {
        // Cannot move folder into itself
        if target_id == folder.id {
            return Err(UpdateFolderError::CannotMoveIntoSelf);
        }

        // Ensure the target folder exists, also ensures the target folder is in the same scope
        // (We may allow across scopes in the future, but would need additional checks for access control of target scope)
        let target_folder = Folder::find_by_id(db.deref_mut(), scope, target_id)
            .await
            .inspect_err(|cause| tracing::error!(?cause, "failed to query target folder"))?
            .ok_or(UpdateFolderError::UnknownTargetFolder)?;

        folder = move_folder(&mut db, user_id.clone(), folder, folder_id, target_folder)
            .await
            .inspect_err(|cause| tracing::error!(?cause, "failed to move folder"))?;
    };

    if let Some(new_name) = update.name {
        folder = update_folder_name(&mut db, user_id, folder, new_name)
            .await
            .inspect_err(|cause| tracing::error!(?cause, "failed to update folder name"))?;
    }

    // Update search index data for the new name and value
    search
        .update_data(
            folder.id,
            UpdateSearchIndexData {
                folder_id: folder.folder_id,
                name: Some(folder.name.clone()),
                content: None,
                pages: None,
            },
        )
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to update search index");
            UpdateFolderError::SearchIndex(cause)
        })?;

    db.commit().await.inspect_err(|cause| {
        tracing::error!(?cause, "failed to commit transaction");
    })?;

    Ok(())
}

#[tracing::instrument(skip_all, fields(?user_id, folder_id = %folder.id, target_folder_id = %target_folder.id))]
async fn move_folder(
    db: &mut DbTransaction<'_>,
    user_id: Option<UserId>,
    folder: Folder,
    folder_id: FolderId,
    target_folder: Folder,
) -> DbResult<Folder> {
    // Track the edit history
    EditHistory::create(
        db.deref_mut(),
        CreateEditHistory {
            ty: CreateEditHistoryType::Folder(folder.id),
            user_id: user_id.clone(),
            metadata: EditHistoryMetadata::MoveToFolder {
                original_id: folder_id,
                target_id: target_folder.id,
            },
        },
    )
    .await
    .inspect_err(|error| tracing::error!(?error, "failed to store folder move edit history"))?;

    // Perform the move
    folder
        .move_to_folder(db.deref_mut(), target_folder.id)
        .await
        .inspect_err(|error| tracing::error!(?error, "failed to move folder"))
}

#[tracing::instrument(skip_all, fields(?user_id, folder_id = %folder.id, %new_name))]
async fn update_folder_name(
    db: &mut DbTransaction<'_>,
    user_id: Option<UserId>,
    folder: Folder,
    new_name: String,
) -> DbResult<Folder> {
    // Track the edit history
    EditHistory::create(
        db.deref_mut(),
        CreateEditHistory {
            ty: CreateEditHistoryType::Folder(folder.id),
            user_id: user_id.clone(),
            metadata: EditHistoryMetadata::Rename {
                original_name: folder.name.clone(),
                new_name: new_name.clone(),
            },
        },
    )
    .await
    .inspect_err(|error| tracing::error!(?error, "failed to store folder rename edit history"))?;

    // Perform the rename
    folder
        .rename(db.deref_mut(), new_name)
        .await
        .inspect_err(|error| tracing::error!(?error, "failed to rename folder in database"))
}
