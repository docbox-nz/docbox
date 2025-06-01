use docbox_database::{
    models::{
        document_box::DocumentBoxScopeRaw,
        edit_history::{
            CreateEditHistory, CreateEditHistoryType, EditHistory, EditHistoryMetadata,
        },
        folder::{Folder, FolderId},
        link::Link,
        user::UserId,
    },
    DbErr, DbPool, DbResult, DbTransaction,
};
use std::ops::DerefMut;
use thiserror::Error;

use crate::search::{models::UpdateSearchIndexData, TenantSearchIndex};

#[derive(Debug, Error)]
pub enum UpdateLinkError {
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

pub struct UpdateLink {
    /// Move the link to another folder
    pub folder_id: Option<FolderId>,

    /// Update the link name
    pub name: Option<String>,

    /// Update the link value
    pub value: Option<String>,
}

pub async fn update_link(
    db: &DbPool,
    search: &TenantSearchIndex,
    scope: &DocumentBoxScopeRaw,
    link: Link,
    user_id: Option<String>,
    update: UpdateLink,
) -> Result<(), UpdateLinkError> {
    let mut link = link;

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
            .ok_or(UpdateLinkError::UnknownTargetFolder)?;

        link = move_link(&mut db, user_id.clone(), link, target_folder)
            .await
            .inspect_err(|cause| tracing::error!(?cause, "failed to move link"))?;
    };

    if let Some(new_name) = update.name {
        link = update_link_name(&mut db, user_id.clone(), link, new_name)
            .await
            .inspect_err(|cause| tracing::error!(?cause, "failed to update link name"))?;
    }

    if let Some(new_value) = update.value {
        link = update_link_value(&mut db, user_id, link, new_value)
            .await
            .inspect_err(|cause| tracing::error!(?cause, "failed to update link value"))?;
    }

    // Update search index data for the new name and value
    search
        .update_data(
            link.id,
            UpdateSearchIndexData {
                folder_id: link.folder_id,
                name: link.name.clone(),
                content: Some(link.value.clone()),
                pages: None,
            },
        )
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to update search index");
            UpdateLinkError::SearchIndex(cause)
        })?;

    db.commit().await.inspect_err(|cause| {
        tracing::error!(?cause, "failed to commit transaction");
    })?;

    Ok(())
}

/// Moves a link to the provided folder, creates a new edit history
/// item for the change
#[tracing::instrument(skip_all, fields(?user_id, link_id = %link.id, target_folder_id = %target_folder.id))]
async fn move_link(
    db: &mut DbTransaction<'_>,
    user_id: Option<UserId>,
    link: Link,
    target_folder: Folder,
) -> DbResult<Link> {
    let link_id = link.id;

    // Track the edit history
    EditHistory::create(
        db.deref_mut(),
        CreateEditHistory {
            ty: CreateEditHistoryType::Link(link.id),
            user_id: user_id.clone(),
            metadata: EditHistoryMetadata::MoveToFolder {
                original_id: link.folder_id,
                target_id: target_folder.id,
            },
        },
    )
    .await
    .inspect_err(|error| {
        tracing::error!(?error, ?link_id, "failed to store link move edit history")
    })?;

    link.move_to_folder(db.deref_mut(), target_folder.id)
        .await
        .inspect_err(|error| tracing::error!(?error, ?link_id, "failed to move link"))
}

/// Updates a link value, creates a new edit history
/// item for the change
#[tracing::instrument(skip_all, fields(?user_id, link_id = %link.id, %new_value))]
async fn update_link_value(
    db: &mut DbTransaction<'_>,
    user_id: Option<UserId>,
    link: Link,
    new_value: String,
) -> DbResult<Link> {
    let link_id = link.id;

    // Track the edit history
    EditHistory::create(
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
    .inspect_err(|cause| {
        tracing::error!(?cause, ?link_id, "failed to store link value edit history")
    })?;

    link.update_value(db.deref_mut(), new_value)
        .await
        .inspect_err(|cause| tracing::error!(?cause, ?link_id, "failed to update link value"))
}

/// Updates a link name, creates a new edit history
/// item for the change
#[tracing::instrument(skip_all, fields(?user_id, link_id = %link.id, %new_name))]
async fn update_link_name(
    db: &mut DbTransaction<'_>,
    user_id: Option<UserId>,
    link: Link,
    new_name: String,
) -> DbResult<Link> {
    let link_id = link.id;

    // Track the edit history
    EditHistory::create(
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
    .inspect_err(|error| {
        tracing::error!(?error, ?link_id, "failed to store link rename edit history")
    })?;

    link.rename(db.deref_mut(), new_name)
        .await
        .inspect_err(|error| tracing::error!(?error, ?link_id, "failed to rename link"))
}
