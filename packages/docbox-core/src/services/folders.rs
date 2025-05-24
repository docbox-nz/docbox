//! Business logic for working with folders

use super::{files::delete_file, links::delete_link};
use crate::{
    events::{TenantEventMessage, TenantEventPublisher},
    search::{
        models::{SearchIndexData, SearchIndexType},
        TenantSearchIndex,
    },
    storage::TenantStorageLayer,
};
use anyhow::Context;
use docbox_database::{
    models::{
        document_box::WithScope,
        edit_history::{
            CreateEditHistory, CreateEditHistoryType, EditHistory, EditHistoryMetadata,
        },
        file::File,
        folder::{CreateFolder, Folder, ResolvedFolder},
        link::Link,
        user::UserId,
    },
    DbErr, DbPool, DbTransaction,
};
use std::{collections::VecDeque, ops::DerefMut};
use thiserror::Error;
use tracing::{debug, error};
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum CreateFolderError {
    /// Failed to start the database transaction
    #[error("failed to begin transaction")]
    BeginTransaction(DbErr),

    /// Failed to create the folder database row
    #[error("failed to create folder: {0}")]
    CreateFolder(DbErr),

    /// Failed to create the search index
    #[error("failed to create folder search index: {0}")]
    CreateIndex(anyhow::Error),

    /// Failed to commit the database transaction
    #[error("failed to commit transaction")]
    CommitTransaction(DbErr),
}

/// State structure to keep track of resources created
/// as a side effect from creating a folder
#[derive(Default)]
pub struct CreateFolderState {
    /// Search index files
    pub search_index_files: Vec<Uuid>,
}

pub struct CreateFolderData {
    /// Folder create the folder within
    pub folder: Folder,

    /// Folder name
    pub name: String,

    /// User creating the link
    pub created_by: Option<UserId>,
}

pub async fn create_folder(
    db: &DbPool,
    search: &TenantSearchIndex,
    events: &TenantEventPublisher,
    create: CreateFolderData,
    create_state: &mut CreateFolderState,
) -> Result<Folder, CreateFolderError> {
    let mut db = db
        .begin()
        .await
        .map_err(CreateFolderError::BeginTransaction)?;

    debug!("creating folder");

    let folder_id = create.folder.id;

    // Create file to commit against
    let folder = Folder::create(
        db.deref_mut(),
        CreateFolder {
            name: create.name,
            document_box: create.folder.document_box,
            folder_id: Some(folder_id),
            created_by: create.created_by,
        },
    )
    .await
    .map_err(CreateFolderError::CreateFolder)?;

    // Add folder to search index
    search
        .add_data(SearchIndexData {
            ty: SearchIndexType::Folder,
            item_id: folder.id,
            folder_id,
            name: folder.name.to_string(),
            mime: None,
            content: None,
            pages: None,
            created_at: folder.created_at.to_rfc3339(),
            created_by: folder.created_by.clone(),
            document_box: folder.document_box.clone(),
        })
        .await
        .map_err(CreateFolderError::CreateIndex)?;

    create_state.search_index_files.push(folder.id);

    db.commit()
        .await
        .map_err(CreateFolderError::CommitTransaction)?;

    // Publish an event
    events.publish_event(TenantEventMessage::FolderCreated(WithScope::new(
        folder.clone(),
        folder.document_box.clone(),
    )));

    Ok(folder)
}

pub async fn rollback_create_folder(search: &TenantSearchIndex, create_state: CreateFolderState) {
    // Revert file index data
    for id in create_state.search_index_files {
        if let Err(err) = search.delete_data(id).await {
            error!(?id, ?err, "failed to rollback created folder search index",);
        }
    }
}

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

pub async fn delete_folder(
    db: &DbPool,
    storage: &TenantStorageLayer,
    search: &TenantSearchIndex,
    events: &TenantEventPublisher,
    folder: Folder,
) -> anyhow::Result<()> {
    // Stack to store the next item to delete
    let mut stack = VecDeque::new();

    let document_box = folder.document_box.clone();

    // Push the first folder item
    stack.push_back(RemoveStackItem::Folder(folder));

    while let Some(item) = stack.pop_front() {
        match item {
            RemoveStackItem::Folder(folder) => {
                // Resolve the folder children
                let resolved = ResolvedFolder::resolve(db, &folder).await?;

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
) -> anyhow::Result<()> {
    // Delete the indexed file contents
    search.delete_data(folder.id).await?;

    folder.delete(db).await?;

    let document_box = folder.document_box.clone();

    // Publish an event
    events.publish_event(TenantEventMessage::FolderDeleted(WithScope::new(
        folder,
        document_box,
    )));

    Ok(())
}

pub async fn move_folder(
    db: &mut DbTransaction<'_>,
    user_id: Option<UserId>,
    folder: Folder,
    target_folder: Folder,
) -> anyhow::Result<Folder> {
    // Track the edit history
    EditHistory::create(
        db.deref_mut(),
        CreateEditHistory {
            ty: CreateEditHistoryType::Folder(folder.id),
            user_id: user_id.clone(),
            metadata: EditHistoryMetadata::MoveToFolder {
                original_id: folder.folder_id,
                target_id: target_folder.id,
            },
        },
    )
    .await
    .context("failed to store move edit history")?;

    folder
        .move_to_folder(db.deref_mut(), target_folder.id)
        .await
        .context("failed to move folder")
}

pub async fn update_folder_name(
    db: &mut DbTransaction<'_>,
    user_id: Option<UserId>,
    folder: Folder,
    new_name: String,
) -> anyhow::Result<Folder> {
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
    .context("failed to store rename edit history")?;

    folder
        .rename(db.deref_mut(), new_name)
        .await
        .context("failed to rename folder")
}

pub async fn re_index_folder(search: &TenantSearchIndex, folder: Folder) -> anyhow::Result<()> {
    let folder_id = match folder.folder_id {
        Some(value) => value,
        // Root folders are not included in the index
        None => return Ok(()),
    };

    // Re-create base folder index
    search
        .add_data(SearchIndexData {
            ty: SearchIndexType::Folder,
            item_id: folder.id,
            folder_id,
            name: folder.name,
            mime: None,
            content: None,
            pages: None,
            created_at: folder.created_at.to_rfc3339(),
            created_by: folder.created_by.clone(),
            document_box: folder.document_box.clone(),
        })
        .await
        .context("failed to create file base index")?;

    Ok(())
}
