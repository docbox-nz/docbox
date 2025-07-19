use crate::events::{TenantEventMessage, TenantEventPublisher};
use docbox_database::{
    DbErr, DbPool, DbResult, DbTransaction,
    models::{
        document_box::{DocumentBox, DocumentBoxScopeRaw},
        folder::{CreateFolder, Folder},
        user::UserId,
    },
};
use std::ops::DerefMut;
use thiserror::Error;

const ROOT_FOLDER_NAME: &str = "Root";

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

/// Create a new document box
pub async fn create_document_box(
    db: &DbPool,
    events: &TenantEventPublisher,
    create: CreateDocumentBox,
) -> Result<(DocumentBox, Folder), CreateDocumentBoxError> {
    // Enter a database transaction
    let mut transaction = db.begin().await?;

    let document_box: DocumentBox =
        create_document_box_entry(&mut transaction, create.scope).await?;
    let root = create_root_folder(
        &mut transaction,
        document_box.scope.clone(),
        create.created_by,
    )
    .await?;

    transaction.commit().await?;

    // Publish an event
    events.publish_event(TenantEventMessage::DocumentBoxCreated(document_box.clone()));

    Ok((document_box, root))
}

/// Create the database entry for the document box itself
async fn create_document_box_entry(
    db: &mut DbTransaction<'_>,
    scope: DocumentBoxScopeRaw,
) -> Result<DocumentBox, CreateDocumentBoxError> {
    DocumentBox::create(db.deref_mut(), scope)
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
        })
}

/// Create the "root" folder within the document box that all
/// the contents will be stored within.
async fn create_root_folder(
    db: &mut DbTransaction<'_>,
    document_box: DocumentBoxScopeRaw,
    created_by: Option<UserId>,
) -> DbResult<Folder> {
    Folder::create(
        db.deref_mut(),
        CreateFolder {
            name: ROOT_FOLDER_NAME.to_string(),
            document_box,
            folder_id: None,
            created_by,
            pinned: false,
        },
    )
    .await
    .inspect_err(|error| tracing::error!(?error, "failed to create document box root folder"))
}
