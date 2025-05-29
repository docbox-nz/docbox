use anyhow::Context;
use docbox_database::{
    models::{
        edit_history::{
            CreateEditHistory, CreateEditHistoryType, EditHistory, EditHistoryMetadata,
        },
        folder::Folder,
        user::UserId,
    },
    DbTransaction,
};
use std::ops::DerefMut;

pub async fn move_folder(
    db: &mut DbTransaction<'_>,
    user_id: Option<UserId>,
    folder: Folder,
    target_folder: Folder,
) -> anyhow::Result<Folder> {
    let folder_id = match folder.folder_id {
        Some(value) => value,
        None => anyhow::bail!("cannot move root file"),
    };

    // Track the edit history
    EditHistory::create(
        db.deref_mut(),
        CreateEditHistory {
            ty: CreateEditHistoryType::Folder(folder.id),
            user_id: user_id.clone(),
            metadata: EditHistoryMetadata::MoveToFolder {
                original_id: folder_id,
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
