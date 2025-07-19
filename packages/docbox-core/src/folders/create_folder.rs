use crate::{
    events::{TenantEventMessage, TenantEventPublisher},
    folders::index_folder::store_folder_index,
};
use docbox_database::{
    DbErr, DbPool,
    models::{
        document_box::WithScope,
        folder::{CreateFolder, Folder},
        user::UserId,
    },
};
use docbox_search::TenantSearchIndex;
use std::ops::DerefMut;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum CreateFolderError {
    /// Database error occurred
    #[error(transparent)]
    Database(#[from] DbErr),

    /// Failed to create the search index
    #[error("failed to create folder search index: {0}")]
    CreateIndex(anyhow::Error),
}

pub struct CreateFolderData {
    /// Folder create the folder within
    pub folder: Folder,

    /// Folder name
    pub name: String,

    /// User creating the link
    pub created_by: Option<UserId>,

    /// Whether the folder should be pinned
    pub pinned: Option<bool>,
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
    tracing::debug!("creating folder");

    let mut db = db
        .begin()
        .await
        .inspect_err(|error| tracing::error!(?error, "failed to being transaction"))?;

    let folder_id = create.folder.id;

    // Create file to commit against
    let folder = Folder::create(
        db.deref_mut(),
        CreateFolder {
            name: create.name,
            document_box: create.folder.document_box,
            folder_id: Some(folder_id),
            created_by: create.created_by,
            pinned: create.pinned.unwrap_or_default(),
        },
    )
    .await
    .inspect_err(|error| tracing::error!(?error, "failed to create folder"))?;

    // Add folder to search index
    store_folder_index(search, &folder, folder_id).await?;
    create_state.search_index_files.push(folder.id);

    db.commit()
        .await
        .inspect_err(|error| tracing::error!(?error, "failed to commit transaction"))?;

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
        if let Err(error) = search.delete_data(id).await {
            tracing::error!(
                ?error, index_id = %id,
                "failed to rollback created folder search index",
            );
        }
    }
}
