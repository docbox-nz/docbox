use crate::{
    events::TenantEventPublisher,
    files::{
        create_file_key,
        upload_file::{
            ProcessingConfig, UploadFile, UploadFileError, UploadFileState, rollback_upload_file,
            upload_file,
        },
    },
    processing::{ProcessingError, ProcessingLayer},
    storage::TenantStorageLayer,
};
use docbox_database::{
    DbErr, DbPool, DbTransaction,
    models::{
        document_box::DocumentBoxScopeRaw,
        file::FileId,
        folder::Folder,
        presigned_upload_task::{
            CreatePresignedUploadTask, PresignedTaskStatus, PresignedUploadTask,
            PresignedUploadTaskId,
        },
        user::UserId,
    },
};
use docbox_search::TenantSearchIndex;
use mime::Mime;
use serde::Serialize;
use std::{collections::HashMap, ops::DerefMut, str::FromStr};
use thiserror::Error;
use uuid::Uuid;

#[derive(Serialize)]
pub struct PresignedUploadOutcome {
    pub task_id: PresignedUploadTaskId,
    pub method: String,
    pub uri: String,
    pub headers: HashMap<String, String>,
}

#[derive(Debug, Error)]
pub enum PresignedUploadError {
    /// Error when uploading files
    #[error(transparent)]
    UploadFile(#[from] UploadFileError),

    /// Error loading the file from S3
    #[error("failed to load file from s3")]
    LoadFile(anyhow::Error),

    /// Stored file metadata mime type was invalid
    #[error("file had an invalid mime type")]
    InvalidMimeType(mime::FromStrError),

    /// Failed to create the file database row
    #[error("failed to create file")]
    CreateFile(DbErr),

    /// Failed to process the file
    #[error("failed to process file: {0}")]
    Processing(#[from] ProcessingError),

    /// Failed to update the task status
    #[error("failed to update task status")]
    UpdateTaskStatus(DbErr),
}

/// State keeping track of whats been generated from a file
/// upload, to help with reverted changes on failure
#[derive(Default)]
pub struct PresignedUploadState {
    /// File upload state
    file: UploadFileState,
}

pub struct CreatePresigned {
    /// Name of the file being uploaded
    pub name: String,

    /// The document box scope to store within
    pub document_box: DocumentBoxScopeRaw,

    /// Folder to store the file in
    pub folder: Folder,

    /// Size of the file being uploaded
    pub size: i32,

    /// Mime type of the file
    pub mime: Mime,

    /// User uploading the file
    pub created_by: Option<UserId>,

    /// Optional parent file ID
    pub parent_id: Option<FileId>,

    /// Config for processing step
    pub processing_config: Option<ProcessingConfig>,
}

/// Create a new presigned file upload request
pub async fn create_presigned_upload(
    db: &DbPool,
    storage: &TenantStorageLayer,
    create: CreatePresigned,
) -> anyhow::Result<PresignedUploadOutcome> {
    let file_key = create_file_key(
        &create.folder.document_box,
        &create.name,
        &create.mime,
        Uuid::new_v4(),
    );
    let (signed_request, expires_at) = storage
        .create_presigned(&file_key, create.size as i64)
        .await?;

    // Encode the processing config for the database
    let processing_config = match &create.processing_config {
        Some(config) => {
            let value = serde_json::to_value(config).map_err(|err| DbErr::Encode(Box::new(err)))?;
            Some(value)
        }
        None => None,
    };

    let task = PresignedUploadTask::create(
        db,
        CreatePresignedUploadTask {
            name: create.name,
            mime: create.mime.to_string(),
            document_box: create.document_box,
            folder_id: create.folder.id,
            size: create.size,
            file_key,
            created_by: create.created_by,
            expires_at,
            parent_id: create.parent_id,
            processing_config,
        },
    )
    .await?;

    Ok(PresignedUploadOutcome {
        task_id: task.id,
        method: signed_request.method().to_string(),
        uri: signed_request.uri().to_string(),
        headers: signed_request
            .headers()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect(),
    })
}

pub struct CompletePresigned {
    pub task: PresignedUploadTask,
    pub folder: Folder,
}

/// Safely performs [upload_file] ensuring that on failure all resources are
/// properly cleaned up
pub async fn safe_complete_presigned(
    db_pool: DbPool,
    search: TenantSearchIndex,
    storage: TenantStorageLayer,
    events: TenantEventPublisher,
    processing: ProcessingLayer,
    mut complete: CompletePresigned,
) -> Result<(), anyhow::Error> {
    // Start a database transaction
    let mut db = db_pool.begin().await.map_err(|cause| {
        tracing::error!(?cause, "failed to begin transaction");
        anyhow::anyhow!("failed to begin transaction")
    })?;

    let mut upload_state = PresignedUploadState::default();

    match complete_presigned(
        &mut db,
        &search,
        &storage,
        &events,
        &processing,
        &mut complete,
        &mut upload_state,
    )
    .await
    {
        Ok(value) => value,
        Err(err) => {
            let error_message = err.to_string();

            // Attempt to rollback any allocated resources in the background
            tokio::spawn(async move {
                if let Err(cause) = db.rollback().await {
                    tracing::error!(?cause, "failed to roll back database transaction");
                }

                // Update the task status
                if let Err(cause) = complete
                    .task
                    .set_status(
                        &db_pool,
                        PresignedTaskStatus::Failed {
                            error: error_message,
                        },
                    )
                    .await
                {
                    tracing::error!(?cause, "failed to set presigned task status to failure");
                }

                rollback_presigned_upload_file(&search, &storage, upload_state).await;
            });

            return Err(anyhow::Error::from(err));
        }
    };

    // Commit the transaction
    if let Err(cause) = db.commit().await {
        tracing::error!(?cause, "failed to commit transaction");

        // Update the task status
        if let Err(cause) = complete
            .task
            .set_status(
                &db_pool,
                PresignedTaskStatus::Failed {
                    error: "Internal server error".to_string(),
                },
            )
            .await
        {
            tracing::error!(?cause, "failed to set presigned task status to failure");
        }

        // Attempt to rollback any allocated resources in the background
        tokio::spawn(async move {
            rollback_presigned_upload_file(&search, &storage, upload_state).await;
        });

        return Err(anyhow::anyhow!("failed to commit transaction"));
    }

    Ok(())
}

/// Completes a presigned file upload
pub async fn complete_presigned(
    db: &mut DbTransaction<'_>,
    search: &TenantSearchIndex,
    storage: &TenantStorageLayer,
    events: &TenantEventPublisher,
    processing: &ProcessingLayer,
    complete: &mut CompletePresigned,
    upload_state: &mut PresignedUploadState,
) -> Result<(), PresignedUploadError> {
    let task = &mut complete.task;

    // Load the file from S3
    let file_bytes = storage
        .get_file(&task.file_key)
        .await
        .map_err(PresignedUploadError::LoadFile)?
        .collect_bytes()
        .await
        .map_err(PresignedUploadError::LoadFile)?;

    // Get the mime type from the task
    let mime = mime::Mime::from_str(&task.mime).map_err(PresignedUploadError::InvalidMimeType)?;

    // Parse task processing config
    let processing_config: Option<ProcessingConfig> = match &task.processing_config {
        Some(value) => match serde_json::from_value(value.0.clone()) {
            Ok(value) => value,
            Err(cause) => {
                tracing::error!(?cause, "failed to deserialize processing config");
                None
            }
        },
        None => None,
    };

    let upload = UploadFile {
        fixed_id: None,
        parent_id: task.parent_id,
        folder_id: complete.folder.id,
        document_box: complete.folder.document_box.clone(),
        name: task.name.clone(),
        mime,
        file_bytes,
        created_by: task.created_by.clone(),
        file_key: Some(task.file_key.clone()),
        processing_config,
    };

    // Perform the upload
    let output = upload_file(
        db,
        search,
        storage,
        events,
        processing,
        upload,
        &mut upload_state.file,
    )
    .await?;

    // Update the task status
    task.set_status(
        db.deref_mut(),
        PresignedTaskStatus::Completed {
            file_id: output.file.id,
        },
    )
    .await
    .map_err(PresignedUploadError::UpdateTaskStatus)?;

    Ok(())
}

/// Performs the process of rolling back a file upload based
/// on the current upload state
pub async fn rollback_presigned_upload_file(
    search: &TenantSearchIndex,
    storage: &TenantStorageLayer,
    upload_state: PresignedUploadState,
) {
    // Revert file state
    rollback_upload_file(search, storage, upload_state.file).await;
}
