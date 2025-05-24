use std::ops::DerefMut;

use docbox_database::{
    models::{
        document_box::DocumentBox,
        folder::{CreateFolder, Folder},
    },
    DbErr, DbPool,
};
use thiserror::Error;

use crate::{
    events::{TenantEventMessage, TenantEventPublisher},
    search::TenantSearchIndex,
    storage::TenantStorageLayer,
};

use super::folders::delete_folder;

#[derive(Debug, Error)]
pub enum CreateDocumentBoxError {
    #[error("document box with matching scope already exists")]
    ScopeAlreadyExists,

    /// Database error occurred
    #[error(transparent)]
    Database(#[from] DbErr),
}

#[derive(Debug)]
pub struct CreateDocumentBox {
    pub scope: String,
    pub created_by: Option<String>,
}

pub async fn create_document_box(
    db: &DbPool,
    events: &TenantEventPublisher,
    create: CreateDocumentBox,
) -> Result<(DocumentBox, Folder), CreateDocumentBoxError> {
    // Enter a database transaction
    let mut transaction = db.begin().await?;

    // Create the document box
    let document_box: DocumentBox =
        DocumentBox::create(transaction.deref_mut(), create.scope.clone())
            .await
            .map_err(|cause| {
                if let Some(db_err) = cause.as_database_error() {
                    // Handle attempts at a duplicate scope creation
                    if db_err.is_unique_violation() {
                        return CreateDocumentBoxError::ScopeAlreadyExists;
                    }
                }

                tracing::error!(?cause, "failed to create document box");
                CreateDocumentBoxError::from(cause)
            })?;

    // Create the root folder
    let root: Folder = Folder::create(
        transaction.deref_mut(),
        CreateFolder {
            name: "Root".to_string(),
            document_box: document_box.scope.clone(),
            folder_id: None,
            created_by: create.created_by,
        },
    )
    .await
    .inspect_err(|cause| {
        tracing::error!(?cause, "failed to create document box root folder");
    })?;

    transaction.commit().await?;

    // Publish an event
    events.publish_event(TenantEventMessage::DocumentBoxCreated(document_box.clone()));

    Ok((document_box, root))
}

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
