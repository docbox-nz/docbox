use docbox_database::{
    models::{
        document_box::DocumentBoxScope,
        edit_history::{
            CreateEditHistory, CreateEditHistoryType, EditHistory, EditHistoryMetadata,
        },
        file::File,
        folder::{Folder, FolderId},
        user::UserId,
    },
    DbErr, DbPool, DbResult, DbTransaction,
};
use std::ops::DerefMut;
use thiserror::Error;

use crate::search::{models::UpdateSearchIndexData, TenantSearchIndex};

#[derive(Debug, Error)]
pub enum UpdateFileError {
    /// Database related error
    #[error(transparent)]
    Database(#[from] DbErr),

    /// Target folder could not be found
    #[error("unknown target folder")]
    UnknownTargetFolder,

    /// Failed to update the search index
    #[error(transparent)]
    SearchIndex(anyhow::Error),
}

pub struct UpdateFile {
    /// Move the file to another folder
    pub folder_id: Option<FolderId>,

    /// Update the file name
    pub name: Option<String>,
}

pub async fn update_file(
    db: &DbPool,
    search: &TenantSearchIndex,
    scope: &DocumentBoxScope,
    file: File,
    user_id: Option<String>,
    update: UpdateFile,
) -> Result<(), UpdateFileError> {
    let mut file = file;

    let mut db = db
        .begin()
        .await
        .inspect_err(|cause| tracing::error!(?cause, "failed to begin transaction"))?;

    if let Some(target_id) = update.folder_id {
        // Ensure the target folder exists, also ensures the target folder is in the same scope
        // (We may allow across scopes in the future, but would need additional checks for access control of target scope)
        let target_folder = Folder::find_by_id(db.deref_mut(), scope, target_id)
            .await
            .inspect_err(|cause| tracing::error!(?cause, "failed to query target folder"))?
            .ok_or(UpdateFileError::UnknownTargetFolder)?;

        file = move_file(&mut db, user_id.clone(), file, target_folder)
            .await
            .inspect_err(|cause| tracing::error!(?cause, "failed to move file"))?;
    };

    if let Some(new_name) = update.name {
        file = update_file_name(&mut db, user_id, file, new_name)
            .await
            .inspect_err(|cause| tracing::error!(?cause, "failed to update file name"))?;
    }

    // Update search index data for the new name and value
    search
        .update_data(
            file.id,
            UpdateSearchIndexData {
                folder_id: Some(file.folder_id),
                name: Some(file.name.clone()),
                // Don't update unchanged
                content: None,
                pages: None,
            },
        )
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to update search index");
            UpdateFileError::SearchIndex(cause)
        })?;

    db.commit().await.inspect_err(|cause| {
        tracing::error!(?cause, "failed to commit transaction");
    })?;

    Ok(())
}

#[tracing::instrument(skip_all, fields(user_id = ?user_id, file_id = %file.id, new_name = %new_name))]
async fn update_file_name(
    db: &mut DbTransaction<'_>,
    user_id: Option<UserId>,
    file: File,
    new_name: String,
) -> DbResult<File> {
    // Track the edit history
    EditHistory::create(
        db.deref_mut(),
        CreateEditHistory {
            ty: CreateEditHistoryType::File(file.id),
            user_id: user_id.clone(),
            metadata: EditHistoryMetadata::Rename {
                original_name: file.name.clone(),
                new_name: new_name.clone(),
            },
        },
    )
    .await
    .inspect_err(|error| tracing::error!(?error, "failed to store file rename edit history"))?;

    file.rename(db.deref_mut(), new_name.clone())
        .await
        .inspect_err(|error| tracing::error!(?error, "failed to rename file in database"))
}

#[tracing::instrument(skip_all, fields(user_id = ?user_id, file_id = %file.id, target_folder_id = %target_folder.id))]
async fn move_file(
    db: &mut DbTransaction<'_>,
    user_id: Option<UserId>,
    file: File,
    target_folder: Folder,
) -> DbResult<File> {
    // Track the edit history
    EditHistory::create(
        db.deref_mut(),
        CreateEditHistory {
            ty: CreateEditHistoryType::File(file.id),
            user_id: user_id.clone(),
            metadata: EditHistoryMetadata::MoveToFolder {
                original_id: file.folder_id,
                target_id: target_folder.id,
            },
        },
    )
    .await
    .inspect_err(|error| tracing::error!(?error, "failed to store file move edit history"))?;

    file.move_to_folder(db.deref_mut(), target_folder.id)
        .await
        .inspect_err(|error| tracing::error!(?error, "failed to move file in database"))
}
