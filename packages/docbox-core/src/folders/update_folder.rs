use docbox_database::{
    DbErr, DbPool, DbResult, DbTransaction,
    models::{
        document_box::DocumentBoxScopeRaw,
        edit_history::{
            CreateEditHistory, CreateEditHistoryType, EditHistory, EditHistoryMetadata,
        },
        folder::{Folder, FolderId},
        shared::WithFullPath,
        user::UserId,
    },
};
use docbox_search::{SearchError, TenantSearchIndex, models::UpdateSearchIndexData};
use std::ops::DerefMut;
use thiserror::Error;

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

    /// Attempted to move a folder into itself
    #[error("cannot move into child of self")]
    CannotMoveIntoChildOfSelf,

    /// Failed to update the search index
    #[error(transparent)]
    SearchIndex(SearchError),
}

pub struct UpdateFolder {
    /// Move the folder to another folder
    pub folder_id: Option<FolderId>,

    /// Update the folder name
    pub name: Option<String>,

    /// Update the pinned state
    pub pinned: Option<bool>,
}

pub async fn update_folder(
    db: &DbPool,
    search: &TenantSearchIndex,
    scope: &DocumentBoxScopeRaw,
    folder: Folder,
    user_id: Option<String>,
    update: UpdateFolder,
) -> Result<(), UpdateFolderError> {
    let mut folder = folder;

    let mut folder_id = folder
        .folder_id
        // Cannot modify the root folder, this is not allowed
        .ok_or(UpdateFolderError::CannotModifyRoot)?;

    let mut db = db
        .begin()
        .await
        .inspect_err(|error| tracing::error!(?error, "failed to begin transaction"))?;

    if let Some(target_id) = update.folder_id {
        // Cannot move folder into itself
        if target_id == folder.id {
            return Err(UpdateFolderError::CannotMoveIntoSelf);
        }

        // Ensure the target folder exists, also ensures the target folder is in the same scope
        // (We may allow across scopes in the future, but would need additional checks for access control of target scope)
        let WithFullPath {
            data: target_folder,
            full_path: target_folder_path,
        } = Folder::find_by_id_with_extra(db.deref_mut(), scope, target_id)
            .await
            .inspect_err(|error| tracing::error!(?error, "failed to query target folder"))?
            .ok_or(UpdateFolderError::UnknownTargetFolder)?;

        // Ensure that we aren't moving the folder into a child of itself
        if target_folder_path
            .iter()
            .any(|segment| segment.id == folder.id)
        {
            return Err(UpdateFolderError::CannotMoveIntoChildOfSelf);
        }

        folder_id = target_folder.folder.id;

        folder = move_folder(
            &mut db,
            user_id.clone(),
            folder,
            folder_id,
            target_folder.folder,
        )
        .await
        .inspect_err(|error| tracing::error!(?error, "failed to move folder"))?;
    };

    if let Some(new_name) = update.name {
        folder = update_folder_name(&mut db, user_id.clone(), folder, new_name)
            .await
            .inspect_err(|error| tracing::error!(?error, "failed to update folder name"))?;
    }

    if let Some(new_value) = update.pinned {
        folder = update_folder_pinned(&mut db, user_id, folder, new_value)
            .await
            .inspect_err(|error| tracing::error!(?error, "failed to update folder pinned state"))?;
    }

    // Update search index data for the new name and value
    search
        .update_data(
            folder.id,
            UpdateSearchIndexData {
                folder_id,
                name: folder.name.clone(),
                content: None,
                pages: None,
            },
        )
        .await
        .map_err(|error| {
            tracing::error!(?error, "failed to update search index");
            UpdateFolderError::SearchIndex(error)
        })?;

    db.commit().await.inspect_err(|error| {
        tracing::error!(?error, "failed to commit transaction");
    })?;

    Ok(())
}

/// Add a new edit history item for a folder
#[tracing::instrument(skip_all, fields(?user_id, %folder_id, ?metadata))]
async fn add_edit_history(
    db: &mut DbTransaction<'_>,
    user_id: Option<UserId>,
    folder_id: FolderId,
    metadata: EditHistoryMetadata,
) -> DbResult<()> {
    // Track the edit history
    EditHistory::create(
        db.deref_mut(),
        CreateEditHistory {
            ty: CreateEditHistoryType::Folder(folder_id),
            user_id,
            metadata,
        },
    )
    .await
    .inspect_err(|error| tracing::error!(?error, "failed to store folder edit history entry"))?;

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
    add_edit_history(
        db,
        user_id,
        folder.id,
        EditHistoryMetadata::MoveToFolder {
            original_id: folder_id,
            target_id: target_folder.id,
        },
    )
    .await?;

    // Perform the move
    folder
        .move_to_folder(db.deref_mut(), target_folder.id)
        .await
        .inspect_err(|error| tracing::error!(?error, "failed to move folder"))
}

#[tracing::instrument(skip_all, fields(?user_id, folder_id = %folder.id, %new_value))]
async fn update_folder_pinned(
    db: &mut DbTransaction<'_>,
    user_id: Option<UserId>,
    folder: Folder,
    new_value: bool,
) -> DbResult<Folder> {
    // Track the edit history
    add_edit_history(
        db,
        user_id,
        folder.id,
        EditHistoryMetadata::ChangePinned {
            previous_value: folder.pinned,
            new_value,
        },
    )
    .await?;

    folder
        .set_pinned(db.deref_mut(), new_value)
        .await
        .inspect_err(|error| tracing::error!(?error, "failed to update folder pinned state"))
}

#[tracing::instrument(skip_all, fields(?user_id, folder_id = %folder.id, %new_name))]
async fn update_folder_name(
    db: &mut DbTransaction<'_>,
    user_id: Option<UserId>,
    folder: Folder,
    new_name: String,
) -> DbResult<Folder> {
    // Track the edit history
    add_edit_history(
        db,
        user_id,
        folder.id,
        EditHistoryMetadata::Rename {
            original_name: folder.name.clone(),
            new_name: new_name.clone(),
        },
    )
    .await?;

    // Perform the rename
    folder
        .rename(db.deref_mut(), new_name)
        .await
        .inspect_err(|error| tracing::error!(?error, "failed to rename folder in database"))
}
