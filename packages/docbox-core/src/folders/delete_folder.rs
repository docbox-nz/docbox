use crate::events::{TenantEventMessage, TenantEventPublisher};
use crate::files::delete_file::{DeleteFileError, delete_file};
use crate::links::delete_link::{DeleteLinkError, delete_link};
use docbox_database::{
    DbPool,
    models::{
        document_box::WithScope,
        file::File,
        folder::{Folder, ResolvedFolder},
        link::Link,
    },
};
use docbox_search::{SearchError, TenantSearchIndex};
use docbox_storage::TenantStorageLayer;
use std::collections::VecDeque;
use thiserror::Error;

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
    storage: &TenantStorageLayer,
    search: &TenantSearchIndex,
    events: &TenantEventPublisher,
    folder: Folder,
) -> Result<(), DeleteFolderError> {
    // Stack to store the next item to delete
    let mut stack = VecDeque::new();

    let document_box = folder.document_box.clone();

    // Push the first folder item
    stack.push_back(RemoveStackItem::Folder(folder));

    while let Some(item) = stack.pop_front() {
        match item {
            RemoveStackItem::Folder(folder) => {
                // Resolve the folder children
                let resolved = ResolvedFolder::resolve(db, folder.id)
                    .await
                    .map_err(|error| {
                        tracing::error!(?error, "failed to resolve folder for deletion");
                        DeleteFolderError::ResolveFolder
                    })?;

                // Push the empty folder first (Will be taken out last)
                stack.push_front(RemoveStackItem::EmptyFolder(folder));

                // Populate the stack with the resolved files, links, and folders
                for item in resolved.folders {
                    stack.push_front(RemoveStackItem::Folder(item));
                }

                for item in resolved.files {
                    stack.push_front(RemoveStackItem::File(item));
                }

                for item in resolved.links {
                    stack.push_front(RemoveStackItem::Link(item));
                }
            }
            RemoveStackItem::EmptyFolder(folder) => {
                internal_delete_folder(db, search, events, folder).await?;
            }
            RemoveStackItem::File(file) => {
                delete_file(db, storage, search, events, file, document_box.clone()).await?;
            }
            RemoveStackItem::Link(link) => {
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
