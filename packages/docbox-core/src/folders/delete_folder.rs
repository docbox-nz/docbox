use crate::events::{TenantEventMessage, TenantEventPublisher};
use crate::files::delete_file::{DeleteFileError, delete_file};
use crate::folders::folder_stream::FolderWalkStream;
use crate::links::delete_link::{DeleteLinkError, delete_link};
use docbox_database::{
    DbPool,
    models::{document_box::WithScope, file::File, folder::Folder, link::Link},
};
use docbox_search::{SearchError, TenantSearchIndex};
use docbox_storage::StorageLayer;
use futures::StreamExt;
use thiserror::Error;

use super::folder_stream::FolderWalkItem;

/// Item to be removed
pub enum RemoveStackItem {
    /// Folder that needs to have its children removed
    Folder(Folder),
    /// Folder that has already been processed, all children
    /// should have been removed by previous stack passes
    EmptyFolder(Folder),
    /// File to remove
    File(File),
    /// Link to remove
    Link(Link),
}

#[derive(Debug, Error)]
pub enum DeleteFolderError {
    #[error("failed to resolve folder for deletion")]
    ResolveFolder,
    #[error(transparent)]
    Folder(#[from] InternalDeleteFolderError),
    #[error(transparent)]
    File(#[from] DeleteFileError),
    #[error(transparent)]
    Link(#[from] DeleteLinkError),
}

#[derive(Debug, Error)]
pub enum InternalDeleteFolderError {
    #[error(transparent)]
    Search(#[from] SearchError),

    #[error("failed to delete folder metadata")]
    Database,
}

pub async fn delete_folder(
    db: &DbPool,
    storage: &StorageLayer,
    search: &TenantSearchIndex,
    events: &TenantEventPublisher,
    folder: Folder,
) -> Result<(), DeleteFolderError> {
    let document_box = folder.document_box.clone();

    let mut stream = FolderWalkStream::new(db, folder);

    while let Some(result) = stream.next().await {
        let item = result.map_err(|error| {
            tracing::error!(?error, "failed to resolve folder for deletion");
            DeleteFolderError::ResolveFolder
        })?;

        match item {
            FolderWalkItem::Folder(folder) => {
                internal_delete_folder(db, search, events, folder).await?;
            }
            FolderWalkItem::File(file) => {
                delete_file(db, storage, search, events, file, document_box.clone()).await?;
            }
            FolderWalkItem::Link(link) => {
                delete_link(db, search, events, link, document_box.clone()).await?;
            }
        }
    }

    Ok(())
}

/// Deletes the folder itself and associated metadata, use [delete_folder]
/// to properly delete the folder and all of its recursive contents
async fn internal_delete_folder(
    db: &DbPool,
    search: &TenantSearchIndex,
    events: &TenantEventPublisher,
    folder: Folder,
) -> Result<(), InternalDeleteFolderError> {
    // Delete the indexed file contents
    search.delete_data(folder.id).await?;

    let result = folder.delete(db).await.map_err(|error| {
        tracing::error!(?error, "failed to delete folder");
        InternalDeleteFolderError::Database
    })?;

    let document_box = folder.document_box.clone();

    // Check we actually removed something before emitting an event
    if result.rows_affected() < 1 {
        return Ok(());
    }

    // Publish an event
    events.publish_event(TenantEventMessage::FolderDeleted(WithScope::new(
        folder,
        document_box,
    )));

    Ok(())
}
