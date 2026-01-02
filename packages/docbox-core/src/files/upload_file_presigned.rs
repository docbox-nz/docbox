use crate::{
    events::TenantEventPublisher,
    files::{
        create_file_key,
        upload_file::{UploadFile, UploadFileError, UploadedFileData, upload_file},
    },
};
use docbox_database::{
    DbErr, DbPool,
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
use docbox_processing::{ProcessingConfig, ProcessingError, ProcessingLayer};
use docbox_search::TenantSearchIndex;
use docbox_storage::{StorageLayerError, TenantStorageLayer};
use mime::Mime;
use serde::Serialize;
use std::{collections::HashMap, str::FromStr};
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

    /// Error loading the file the storage layer
    #[error("failed to load file from storage")]
    LoadFile(StorageLayerError),

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

#[derive(Debug, Error)]
pub enum CreatePresignedUploadError {
    #[error("failed to create presigned url")]
    CreatePresigned,

    #[error("failed to store upload configuration")]
    SerializeConfig,

    #[error("failed to store presigned upload task")]
    StoreTask,
}

/// Create a new presigned file upload request
pub async fn create_presigned_upload(
    db: &DbPool,
    storage: &TenantStorageLayer,
    create: CreatePresigned,
) -> Result<PresignedUploadOutcome, CreatePresignedUploadError> {
    let file_key = create_file_key(
        &create.folder.document_box,
        &create.name,
        &create.mime,
        Uuid::new_v4(),
    );
    let (signed_request, expires_at) = storage
        .create_presigned(&file_key, create.size as i64)
        .await
        .map_err(|error| {
            tracing::error!(?error, "failed to create presigned upload");
            CreatePresignedUploadError::CreatePresigned
        })?;

    // Encode the processing config for the database
    let processing_config = match &create.processing_config {
        Some(config) => {
            let value = serde_json::to_value(config).map_err(|error| {
                tracing::error!(?error, "failed to serialize processing config");
                CreatePresignedUploadError::SerializeConfig
            })?;

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
    .await
    .map_err(|error| {
        tracing::error!(?error, "failed to store presigned upload task");
        CreatePresignedUploadError::StoreTask
    })?;

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
) -> Result<(), PresignedUploadError> {
    match complete_presigned(
        &db_pool,
        &search,
        &storage,
        &processing,
        &events,
        &mut complete,
    )
    .await
    {
        Ok(output) => {
            let status = PresignedTaskStatus::Completed {
                file_id: output.file.id,
            };

            if let Err(error) = complete.task.set_status(&db_pool, status).await {
                tracing::error!(?error, "failed to set presigned task status");
                return Err(PresignedUploadError::UpdateTaskStatus(error));
            }

            Ok(())
        }
        Err(error) => {
            tracing::error!(?error, "failed to complete presigned upload");
            let status = PresignedTaskStatus::Failed {
                error: error.to_string(),
            };

            if let Err(error) = complete.task.set_status(&db_pool, status).await {
                tracing::error!(?error, "failed to set presigned task status");
                return Err(PresignedUploadError::UpdateTaskStatus(error));
            }

            Err(error)
        }
    }
}

/// Completes a presigned file upload
pub async fn complete_presigned(
    db: &DbPool,
    search: &TenantSearchIndex,
    storage: &TenantStorageLayer,
    processing: &ProcessingLayer,
    events: &TenantEventPublisher,
    complete: &mut CompletePresigned,
) -> Result<UploadedFileData, PresignedUploadError> {
    let task = &mut complete.task;

    // Load the file from storage
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
            Err(error) => {
                tracing::error!(?error, "failed to deserialize processing config");
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
    let output = upload_file(db, search, storage, processing, events, upload).await?;
    Ok(output)
}
