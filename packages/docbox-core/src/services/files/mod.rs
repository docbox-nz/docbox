//! Business logic for working with files

use std::ops::DerefMut;

use crate::{
    events::{TenantEventMessage, TenantEventPublisher},
    search::TenantSearchIndex,
    storage::TenantStorageLayer,
    utils::file::{get_file_name_ext, get_mime_ext, make_s3_safe},
};
use docbox_database::{
    models::{
        document_box::{DocumentBoxScope, WithScope},
        edit_history::{
            CreateEditHistory, CreateEditHistoryType, EditHistory, EditHistoryMetadata,
        },
        file::File,
        folder::Folder,
        generated_file::GeneratedFile,
        user::UserId,
    },
    DbErr, DbPool, DbTransaction,
};
use futures::{stream::FuturesUnordered, StreamExt};
use mime::Mime;
use thiserror::Error;
use tracing::error;
use uuid::Uuid;

use super::generated::{delete_generated_files, GeneratedFileDeleteResult};

pub mod indexing;
pub mod presigned;
pub mod upload;

pub fn create_file_key(document_box: &DocumentBoxScope, name: &str, mime: &Mime) -> String {
    // Try get file extension from name
    let file_ext = get_file_name_ext(name)
        // Fallback to extension from mime type
        .or_else(|| get_mime_ext(mime).map(|value| value.to_string()))
        // Fallback to default .bin extension
        .unwrap_or_else(|| "bin".to_string());

    // Get the file name with the file extension stripped
    let file_name = name.strip_suffix(&file_ext).unwrap_or(name);

    // Strip unwanted characters from the file name
    let clean_file_name = make_s3_safe(file_name);

    // Unique portion of the file key
    let file_key_unique = Uuid::new_v4().to_string();

    // Key is composed of the {Unique ID}_{File Name}.{File Ext}
    let file_key = format!("{file_key_unique}_{clean_file_name}.{file_ext}");

    // Prefix file key with the scope directory
    format!("{}/{}", document_box, file_key)
}

#[derive(Debug, Error)]
pub enum DeleteFileError {
    /// Failed to delete the search index
    #[error("failed to delete tenant search index: {0}")]
    DeleteIndex(anyhow::Error),

    /// Failed to find generated files
    #[error("failed to query generated files: {0}")]
    GetGeneratedFiles(DbErr),

    /// Failed to delete the file database row
    #[error("failed to create file: {0}")]
    DeleteFile(DbErr),

    /// Failed to remove file from s3
    #[error("failed to remove file from s3: {0}")]
    DeleteFileS3(anyhow::Error),

    /// Failed to remove generated file from s3
    #[error("failed to remove generated file from s3: {0}")]
    DeleteGeneratedS3(anyhow::Error),

    /// Failed to delete the generated file database row
    #[error("failed to create generated file: {0}")]
    DeleteGeneratedFile(DbErr),
}

/// Deletes a file and all associated generated files.
///
/// Deletes files from S3 before deleting the database metadata to
/// prevent dangling files in the bucket. Same goes for the search
/// index
///
/// This process cannot be rolled back since the changes to S3 are
/// permanent. If a failure occurs after generated files are deleted
/// they will stay deleted.
///
/// We may choose to revise this to load the generated files into memory
/// in order to restore them on failure.
pub async fn delete_file(
    db: &DbPool,
    storage: &TenantStorageLayer,
    search: &TenantSearchIndex,
    events: &TenantEventPublisher,
    file: File,
    scope: DocumentBoxScope,
) -> Result<(), DeleteFileError> {
    let generated = GeneratedFile::find_all(db, file.id)
        .await
        .map_err(DeleteFileError::GetGeneratedFiles)?;

    match delete_generated_files(storage, &generated).await {
        GeneratedFileDeleteResult::Ok => {}
        GeneratedFileDeleteResult::Err(deleted, err) => {
            // Attempt to delete generated files from db that were deleted from S3
            let mut delete_files_future = generated
                .into_iter()
                .filter(|file| deleted.contains(&file.id))
                .map(|file| file.delete(db))
                .collect::<FuturesUnordered<_>>();

            // Ignore errors from this point, they are not recoverable
            while let Some(result) = delete_files_future.next().await {
                if let Err(cause) = result {
                    error!(?cause, "failed to delete generated file from db");
                }
            }

            return Err(DeleteFileError::DeleteGeneratedS3(err));
        }
    }

    let mut delete_files_future = generated
        .into_iter()
        .map(|file| file.delete(db))
        .collect::<FuturesUnordered<_>>();

    // Delete the generated files from the database
    while let Some(result) = delete_files_future.next().await {
        if let Err(cause) = result {
            error!(?cause, "failed to delete generated file");
            return Err(DeleteFileError::DeleteGeneratedFile(cause));
        }
    }

    // Delete the file from S3
    storage
        .delete_file(&file.file_key)
        .await
        .map_err(DeleteFileError::DeleteFileS3)?;

    // Delete the indexed file contents
    search
        .delete_data(file.id)
        .await
        .map_err(DeleteFileError::DeleteIndex)?;

    // Delete the file itself
    file.delete(db).await.map_err(DeleteFileError::DeleteFile)?;

    // Publish an event
    events.publish_event(TenantEventMessage::FileDeleted(WithScope::new(file, scope)));

    Ok(())
}

pub async fn move_file(
    db: &mut DbTransaction<'_>,
    user_id: Option<UserId>,
    file: File,
    target_folder: Folder,
) -> anyhow::Result<File> {
    // Track the edit history
    if let Err(cause) = EditHistory::create(
        db.deref_mut(),
        CreateEditHistory {
            ty: CreateEditHistoryType::File(file.id),
            user_id: user_id.clone(),
            metadata: EditHistoryMetadata::MoveToFolder {
                original_id: Some(file.folder_id),
                target_id: target_folder.id,
            },
        },
    )
    .await
    {
        tracing::error!(?cause, "failed to store file move edit history");
        anyhow::bail!("failed to store move edit history");
    }

    let file = match file.move_to_folder(db.deref_mut(), target_folder.id).await {
        Ok(file) => file,
        Err(cause) => {
            tracing::error!(?cause, "failed to move file in database");
            anyhow::bail!("failed to move file in database");
        }
    };

    Ok(file)
}

pub async fn update_file_name(
    db: &mut DbTransaction<'_>,
    user_id: Option<UserId>,
    file: File,
    new_name: String,
) -> anyhow::Result<File> {
    // Track the edit history
    if let Err(cause) = EditHistory::create(
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
    {
        tracing::error!(?cause, "failed to store file rename edit history");
        anyhow::bail!("failed to store rename edit history");
    }

    let file = match file.rename(db.deref_mut(), new_name.clone()).await {
        Ok(file) => file,
        Err(cause) => {
            tracing::error!(?cause, "failed to rename file in database");
            anyhow::bail!("failed to rename file in database");
        }
    };

    Ok(file)
}
