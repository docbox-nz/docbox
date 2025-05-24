//! Business logic for working with links

use std::ops::DerefMut;

use crate::{
    events::{TenantEventMessage, TenantEventPublisher},
    search::{
        models::{SearchIndexData, SearchIndexType},
        TenantSearchIndex,
    },
};
use docbox_database::{
    models::{
        document_box::{DocumentBoxScope, WithScope},
        edit_history::{
            CreateEditHistory, CreateEditHistoryType, EditHistory, EditHistoryMetadata,
        },
        folder::Folder,
        link::{CreateLink as DbCreateLink, Link},
        user::UserId,
    },
    DbErr, DbPool, DbTransaction,
};
use thiserror::Error;
use tracing::{debug, error};
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum CreateLinkError {
    /// Failed to start the database transaction
    #[error("failed to begin transaction")]
    BeginTransaction(DbErr),

    /// Failed to create the link database row
    #[error("failed to create link: {0}")]
    CreateLink(anyhow::Error),

    /// Failed to create the search index
    #[error("failed to create link search index: {0}")]
    CreateIndex(anyhow::Error),

    /// Failed to commit the database transaction
    #[error("failed to commit transaction")]
    CommitTransaction(DbErr),
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
    let mut db = db
        .begin()
        .await
        .map_err(CreateLinkError::BeginTransaction)?;

    debug!("creating link");

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
    .map_err(CreateLinkError::CreateLink)?;

    // Add link to search index
    search
        .add_data(SearchIndexData {
            ty: SearchIndexType::Link,
            item_id: link.id,
            folder_id: link.folder_id,
            name: link.name.to_string(),
            mime: None,
            content: Some(link.value.clone()),
            pages: None,
            created_at: link.created_at.to_rfc3339(),
            created_by: link.created_by.clone(),
            document_box: create.folder.document_box.clone(),
        })
        .await
        .map_err(CreateLinkError::CreateIndex)?;

    create_state.search_index_files.push(link.id);

    db.commit()
        .await
        .map_err(CreateLinkError::CommitTransaction)?;

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
        if let Err(err) = search.delete_data(id).await {
            error!(
                "failed to rollback created link search index: {} {}",
                id, err
            );
        }
    }
}

pub async fn delete_link(
    db: &DbPool,
    search: &TenantSearchIndex,
    events: &TenantEventPublisher,
    link: Link,
    scope: DocumentBoxScope,
) -> anyhow::Result<()> {
    // Delete the indexed file contents
    search.delete_data(link.id).await?;

    let link_id = link.id;

    // Delete the link itself from the db
    if let Err(cause) = link.delete(db).await {
        tracing::error!(?cause, ?link_id, "failed to delete link");
        anyhow::bail!("failed to delete link");
    }

    // Publish an event
    events.publish_event(TenantEventMessage::LinkDeleted(WithScope::new(link, scope)));

    Ok(())
}

/// Moves a link to the provided folder, creates a new edit history
/// item for the change
pub async fn move_link(
    db: &mut DbTransaction<'_>,
    user_id: Option<UserId>,
    link: Link,
    target_folder: Folder,
) -> anyhow::Result<Link> {
    let link_id = link.id;

    // Track the edit history
    if let Err(cause) = EditHistory::create(
        db.deref_mut(),
        CreateEditHistory {
            ty: CreateEditHistoryType::Link(link.id),
            user_id: user_id.clone(),
            metadata: EditHistoryMetadata::MoveToFolder {
                original_id: Some(link.folder_id),
                target_id: target_folder.id,
            },
        },
    )
    .await
    {
        tracing::error!(?cause, ?link_id, "failed to store link move edit history");
        anyhow::bail!("failed to store link move edit history");
    };

    match link.move_to_folder(db.deref_mut(), target_folder.id).await {
        Ok(value) => Ok(value),
        Err(cause) => {
            tracing::error!(?cause, ?link_id, "failed to move link");
            anyhow::bail!("failed to move link");
        }
    }
}

/// Updates a link value, creates a new edit history
/// item for the change
pub async fn update_link_value(
    db: &mut DbTransaction<'_>,
    user_id: Option<UserId>,
    link: Link,
    new_value: String,
) -> anyhow::Result<Link> {
    let link_id = link.id;

    // Track the edit history
    if let Err(cause) = EditHistory::create(
        db.deref_mut(),
        CreateEditHistory {
            ty: CreateEditHistoryType::Link(link.id),
            user_id: user_id.clone(),
            metadata: EditHistoryMetadata::LinkValue {
                previous_value: link.value.clone(),
                new_value: new_value.clone(),
            },
        },
    )
    .await
    {
        tracing::error!(?cause, ?link_id, "failed to store link value edit history");
        anyhow::bail!("failed to store link value edit history");
    }

    match link.update_value(db.deref_mut(), new_value).await {
        Ok(value) => Ok(value),
        Err(cause) => {
            tracing::error!(?cause, ?link_id, "failed to update link value");
            anyhow::bail!("failed to update link value");
        }
    }
}

/// Updates a link name, creates a new edit history
/// item for the change
pub async fn update_link_name(
    db: &mut DbTransaction<'_>,
    user_id: Option<UserId>,
    link: Link,
    new_name: String,
) -> anyhow::Result<Link> {
    let link_id = link.id;

    // Track the edit history
    if let Err(cause) = EditHistory::create(
        db.deref_mut(),
        CreateEditHistory {
            ty: CreateEditHistoryType::Link(link.id),
            user_id: user_id.clone(),
            metadata: EditHistoryMetadata::Rename {
                original_name: link.name.clone(),
                new_name: new_name.clone(),
            },
        },
    )
    .await
    {
        tracing::error!(?cause, ?link_id, "failed to store link rename edit history");
        anyhow::bail!("failed to store link rename edit history");
    };

    match link.rename(db.deref_mut(), new_name).await {
        Ok(value) => Ok(value),
        Err(cause) => {
            tracing::error!(?cause, ?link_id, "failed to rename link");
            anyhow::bail!("failed to rename link");
        }
    }
}

pub async fn re_index_link(
    search: &TenantSearchIndex,
    scope: &DocumentBoxScope,
    link: Link,
) -> anyhow::Result<()> {
    // Re-create base link index
    if let Err(cause) = search
        .add_data(SearchIndexData {
            ty: SearchIndexType::Link,
            item_id: link.id,
            folder_id: link.folder_id,
            name: link.name.to_string(),
            mime: None,
            content: Some(link.value.clone()),
            pages: None,
            created_at: link.created_at.to_rfc3339(),
            created_by: link.created_by.clone(),
            document_box: scope.clone(),
        })
        .await
    {
        tracing::error!(?cause, "failed to create file base index");
        anyhow::bail!("failed to create file base index");
    }

    Ok(())
}
