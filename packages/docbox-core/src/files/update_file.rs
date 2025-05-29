use docbox_database::{
    models::{
        edit_history::{
            CreateEditHistory, CreateEditHistoryType, EditHistory, EditHistoryMetadata,
        },
        file::File,
        folder::Folder,
        user::UserId,
    },
    DbTransaction,
};
use std::ops::DerefMut;

pub async fn update_file_name(
    db: &mut DbTransaction<'_>,
    user_id: Option<UserId>,
    file: File,
    new_name: String,
) -> anyhow::Result<File> {
    // Track the edit history
    if let Err(cause) = EditHistory::create(
        db.deref_mut(),
        CreateEditHistory {
            ty: CreateEditHistoryType::File(file.id),
            user_id: user_id.clone(),
            metadata: EditHistoryMetadata::Rename {
                original_name: file.name.clone(),
                new_name: new_name.clone(),
            },
        },
    )
    .await
    {
        tracing::error!(?cause, "failed to store file rename edit history");
        anyhow::bail!("failed to store rename edit history");
    }

    let file = match file.rename(db.deref_mut(), new_name.clone()).await {
        Ok(file) => file,
        Err(cause) => {
            tracing::error!(?cause, "failed to rename file in database");
            anyhow::bail!("failed to rename file in database");
        }
    };

    Ok(file)
}

pub async fn move_file(
    db: &mut DbTransaction<'_>,
    user_id: Option<UserId>,
    file: File,
    target_folder: Folder,
) -> anyhow::Result<File> {
    // Track the edit history
    if let Err(cause) = EditHistory::create(
        db.deref_mut(),
        CreateEditHistory {
            ty: CreateEditHistoryType::File(file.id),
            user_id: user_id.clone(),
            metadata: EditHistoryMetadata::MoveToFolder {
                original_id: file.folder_id,
                target_id: target_folder.id,
            },
        },
    )
    .await
    {
        tracing::error!(?cause, "failed to store file move edit history");
        anyhow::bail!("failed to store move edit history");
    }

    let file = match file.move_to_folder(db.deref_mut(), target_folder.id).await {
        Ok(file) => file,
        Err(cause) => {
            tracing::error!(?cause, "failed to move file in database");
            anyhow::bail!("failed to move file in database");
        }
    };

    Ok(file)
}
