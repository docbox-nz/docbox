use crate::{
    events::{TenantEventMessage, TenantEventPublisher},
    files::generated::{delete_generated_files, GeneratedFileDeleteResult},
    search::TenantSearchIndex,
    storage::TenantStorageLayer,
};
use docbox_database::{
    models::{
        document_box::{DocumentBoxScope, WithScope},
        file::File,
        generated_file::GeneratedFile,
    },
    DbErr, DbPool,
};
use futures::{stream::FuturesUnordered, StreamExt};
use thiserror::Error;
use tracing::error;

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

    /// Failed to remove file from storage
    #[error("failed to remove file from storage: {0}")]
    DeleteFileStorage(anyhow::Error),

    /// Failed to remove generated file from storage
    #[error("failed to remove generated file from storage: {0}")]
    DeleteGeneratedFileStorage(anyhow::Error),

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

            return Err(DeleteFileError::DeleteGeneratedFileStorage(err));
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
        .map_err(DeleteFileError::DeleteFileStorage)?;

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
