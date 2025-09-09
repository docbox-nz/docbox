use crate::{
    events::{TenantEventMessage, TenantEventPublisher},
    folders::delete_folder::delete_folder,
};
use docbox_database::{
    DbErr, DbPool,
    models::{document_box::DocumentBox, folder::Folder},
};
use docbox_search::TenantSearchIndex;
use docbox_storage::TenantStorageLayer;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DeleteDocumentBoxError {
    /// Database error occurred
    #[error(transparent)]
    Database(#[from] DbErr),

    #[error("unknown document box scope")]
    UnknownScope,

    #[error(transparent)]
    DeleteSearchData(anyhow::Error),

    #[error("failed to delete root folder")]
    FailedToDeleteRoot(anyhow::Error),
}

pub async fn delete_document_box(
    db: &DbPool,
    search: &TenantSearchIndex,
    storage: &TenantStorageLayer,
    events: &TenantEventPublisher,
    scope: String,
) -> Result<(), DeleteDocumentBoxError> {
    let document_box = DocumentBox::find_by_scope(db, &scope)
        .await?
        .ok_or(DeleteDocumentBoxError::UnknownScope)?;

    let root = Folder::find_root(db, &scope).await?;

    if let Some(root) = root {
        // Delete root folder
        delete_folder(db, storage, search, events, root)
            .await
            .inspect_err(|error| tracing::error!(?error, "failed to delete bucket root folder"))
            .map_err(DeleteDocumentBoxError::FailedToDeleteRoot)?;
    }

    // Delete document box
    let result = document_box.delete(db).await?;

    // Check we actually removed something before emitting an event
    if result.rows_affected() < 1 {
        return Ok(());
    }

    search
        .delete_by_scope(scope)
        .await
        .map_err(DeleteDocumentBoxError::DeleteSearchData)?;

    // Publish an event
    events.publish_event(TenantEventMessage::DocumentBoxDeleted(document_box));

    Ok(())
}
