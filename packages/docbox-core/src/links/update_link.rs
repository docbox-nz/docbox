use docbox_database::{
    models::{
        edit_history::{
            CreateEditHistory, CreateEditHistoryType, EditHistory, EditHistoryMetadata,
        },
        folder::Folder,
        link::Link,
        user::UserId,
    },
    DbTransaction,
};
use std::ops::DerefMut;

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
                original_id: link.folder_id,
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
