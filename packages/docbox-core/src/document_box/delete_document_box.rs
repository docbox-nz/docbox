use crate::{
    events::{TenantEventMessage, TenantEventPublisher},
    folders::delete_folder::delete_folder,
    storage::TenantStorageLayer,
};
use docbox_database::{
    models::{document_box::DocumentBox, folder::Folder},
    DbErr, DbPool,
};
use docbox_search::TenantSearchIndex;
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
    FailedToDeleteRoot,
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
        if let Err(cause) = delete_folder(db, storage, search, events, root).await {
            tracing::error!(?cause, "failed to delete bucket root folder");
            return Err(DeleteDocumentBoxError::FailedToDeleteRoot);
        };
    }

    // Delete document box
    document_box.delete(db).await?;

    search
        .delete_by_scope(scope)
        .await
        .map_err(DeleteDocumentBoxError::DeleteSearchData)?;

    // Publish an event
    events.publish_event(TenantEventMessage::DocumentBoxDeleted(document_box));

    Ok(())
}
