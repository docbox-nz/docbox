use crate::{
    events::{TenantEventMessage, TenantEventPublisher},
    folders::index_folder::store_folder_index,
    search::TenantSearchIndex,
};
use docbox_database::{
    models::{
        document_box::WithScope,
        folder::{CreateFolder, Folder},
        user::UserId,
    },
    DbErr, DbPool,
};
use std::ops::DerefMut;
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

pub struct CreateFolderData {
    /// Folder create the folder within
    pub folder: Folder,

    /// Folder name
    pub name: String,

    /// User creating the link
    pub created_by: Option<UserId>,
}

/// State structure to keep track of resources created
/// as a side effect from creating a folder
#[derive(Default)]
struct CreateFolderState {
    /// Search index files
    pub search_index_files: Vec<Uuid>,
}

pub async fn safe_create_folder(
    db: &DbPool,
    search: TenantSearchIndex,
    events: &TenantEventPublisher,
    create: CreateFolderData,
) -> Result<Folder, CreateFolderError> {
    let mut create_state = CreateFolderState::default();

    create_folder(db, &search, events, create, &mut create_state)
        .await
        .inspect_err(|_| {
            // Attempt to rollback any allocated resources in the background
            tokio::spawn(rollback_create_folder(search, create_state));
        })
}

async fn create_folder(
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
    store_folder_index(search, &folder, folder_id).await?;
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

async fn rollback_create_folder(search: TenantSearchIndex, create_state: CreateFolderState) {
    // Revert file index data
    for id in create_state.search_index_files {
        if let Err(err) = search.delete_data(id).await {
            error!(?id, ?err, "failed to rollback created folder search index",);
        }
    }
}
