use crate::{
    events::{TenantEventMessage, TenantEventPublisher},
    links::index_link::store_link_index,
};
use docbox_database::{
    DbErr, DbPool,
    models::{
        document_box::WithScope,
        folder::Folder,
        link::{CreateLink as DbCreateLink, Link},
        user::UserId,
    },
};
use docbox_search::{SearchError, TenantSearchIndex};
use std::ops::DerefMut;
use thiserror::Error;
use tracing::error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum CreateLinkError {
    /// Database error occurred
    #[error(transparent)]
    Database(#[from] DbErr),

    /// Failed to create the search index
    #[error("failed to create link search index: {0}")]
    CreateIndex(SearchError),
}

/// State structure to keep track of resources created
/// as a side effect from a link
#[derive(Default)]
struct CreateLinkState {
    /// Search index files
    pub search_index_files: Vec<Uuid>,
}

pub struct CreateLinkData {
    /// Folder to upload the link into
    pub folder: Folder,

    /// Link name
    pub name: String,

    /// Link value
    pub value: String,

    /// User creating the link
    pub created_by: Option<UserId>,
}

/// Safely perform [create_link] ensuring that if an error
/// occurs the changes will be properly rolled back
pub async fn safe_create_link(
    db: &DbPool,
    search: TenantSearchIndex,
    events: &TenantEventPublisher,
    create: CreateLinkData,
) -> Result<Link, CreateLinkError> {
    let mut create_state = CreateLinkState::default();
    create_link(db, &search, events, create, &mut create_state)
        .await
        .inspect_err(|_| {
            // Attempt to rollback any allocated resources in the background
            tokio::spawn(rollback_create_link(search, create_state));
        })
}

async fn create_link(
    db: &DbPool,
    search: &TenantSearchIndex,
    events: &TenantEventPublisher,
    create: CreateLinkData,
    create_state: &mut CreateLinkState,
) -> Result<Link, CreateLinkError> {
    tracing::debug!("creating link");

    let mut db = db
        .begin()
        .await
        .inspect_err(|error| tracing::error!(?error, "failed to being transaction"))?;

    // Create link
    let link = Link::create(
        db.deref_mut(),
        DbCreateLink {
            name: create.name,
            value: create.value,
            folder_id: create.folder.id,
            created_by: create.created_by,
        },
    )
    .await
    .inspect_err(|error| tracing::error!(?error, "failed to create link"))?;

    // Add link to search index
    store_link_index(search, &link, &create.folder.document_box).await?;
    create_state.search_index_files.push(link.id);

    db.commit()
        .await
        .inspect_err(|error| tracing::error!(?error, "failed to commit transaction"))?;

    // Publish an event
    events.publish_event(TenantEventMessage::LinkCreated(WithScope::new(
        link.clone(),
        create.folder.document_box,
    )));

    Ok(link)
}

async fn rollback_create_link(search: TenantSearchIndex, create_state: CreateLinkState) {
    // Revert file index data
    for id in create_state.search_index_files {
        if let Err(error) = search.delete_data(id).await {
            tracing::error!(
                ?error, index_id = %id,
                "failed to rollback created link search index"
            );
        }
    }
}
