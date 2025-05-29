use crate::files::{create_file_key, index_file::store_file_index};
use crate::processing::{process_file, ProcessingError, ProcessingIndexMetadata, ProcessingLayer};
use crate::search::TenantSearchIndex;
use crate::storage::TenantStorageLayer;
use crate::utils::error::CompositeError;
use crate::{
    events::{TenantEventMessage, TenantEventPublisher},
    files::generated::{upload_generated_files, QueuedUpload},
};
use bytes::Bytes;
use docbox_database::models::document_box::DocumentBoxScope;
use docbox_database::models::folder::FolderId;
use docbox_database::{
    models::{
        document_box::WithScope,
        file::{CreateFile, File, FileId},
        generated_file::GeneratedFile,
        user::UserId,
    },
    DbErr, DbPool, DbTransaction,
};
use mime::Mime;
use serde::{Deserialize, Serialize};
use std::ops::DerefMut;
use thiserror::Error;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum UploadFileError {
    /// Failed to create the search index
    #[error("failed to create file search index: {0}")]
    CreateIndex(anyhow::Error),

    /// Failed to create the file database row
    #[error("failed to create file")]
    CreateFile(DbErr),

    /// Failed to set file encryption state
    #[error("failed to store file encryption state")]
    SetEncryption(DbErr),

    /// Failed to process the file
    #[error("failed to process file: {0}")]
    Processing(#[from] ProcessingError),

    /// Failed to upload generated file to storage layer
    #[error("failed to upload generated file to storage layer: {0}")]
    UploadGeneratedFile(anyhow::Error),

    /// Failed to create the generated file database row
    #[error("failed to create generated file")]
    CreateGeneratedFile(DbErr),

    /// Failed to upload file to storage layer
    #[error("failed to upload file to storage layer: {0}")]
    UploadFile(anyhow::Error),

    /// Multiple error messages
    #[error(transparent)]
    Composite(#[from] CompositeError),
}

/// State keeping track of whats been generated from a file
/// upload, to help with reverted changes on failure
#[derive(Default)]
pub struct UploadFileState {
    /// S3 file upload keys
    pub s3_upload_keys: Vec<String>,
    /// Search index files
    pub search_index_files: Vec<Uuid>,
}

pub struct UploadFile {
    /// Fixed file ID to use instead of a randomly
    /// generated file ID
    pub fixed_id: Option<FileId>,

    /// ID of the parent file if this file is related to another file
    pub parent_id: Option<FileId>,

    /// ID of the destination folder
    pub folder_id: FolderId,

    /// Document box the file and folder are contained within
    pub document_box: DocumentBoxScope,

    /// File name
    pub name: String,

    /// File content type
    pub mime: Mime,

    /// File content
    pub file_bytes: Bytes,

    /// User uploading the file
    pub created_by: Option<UserId>,

    /// Key to the file if the file is already uploaded to S3
    pub file_key: Option<String>,

    /// Config that can be used when processing for additional
    /// configuration to how the file is processed
    pub processing_config: Option<ProcessingConfig>,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize, ToSchema)]
#[serde(default)]
pub struct ProcessingConfig {
    /// Email specific processing configuration
    pub email: Option<EmailProcessingConfig>,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize, ToSchema)]
#[serde(default)]
pub struct EmailProcessingConfig {
    /// Whether to skip extracting attachments when processing an email
    pub skip_attachments: Option<bool>,
}

pub struct UploadedFileData {
    /// The uploaded file itself
    pub file: File,
    /// Generated data alongside the file
    pub generated: Vec<GeneratedFile>,
    /// Additional files created and uploaded from processing the file
    pub additional_files: Vec<UploadedFileData>,
}

/// Safely performs [upload_file] ensuring that on failure all resources are
/// properly cleaned up
pub async fn safe_upload_file(
    db: DbPool,
    search: TenantSearchIndex,
    storage: TenantStorageLayer,
    events: TenantEventPublisher,
    processing: ProcessingLayer,
    upload: UploadFile,
) -> Result<UploadedFileData, anyhow::Error> {
    // Start a database transaction
    let mut db = db.begin().await.map_err(|cause| {
        tracing::error!(?cause, "failed to begin transaction");
        anyhow::anyhow!("failed to begin transaction")
    })?;

    // Create state for tracking allocated resources
    let mut upload_state = UploadFileState::default();

    let output = match upload_file(
        &mut db,
        &search,
        &storage,
        &events,
        &processing,
        upload,
        &mut upload_state,
    )
    .await
    {
        Ok(value) => value,
        Err(err) => {
            // Attempt to rollback any allocated resources in the background
            tokio::spawn(async move {
                if let Err(cause) = db.rollback().await {
                    tracing::error!(?cause, "failed to roll back database transaction");
                }

                rollback_upload_file(&search, &storage, upload_state).await;
            });

            return Err(anyhow::Error::from(err));
        }
    };

    // Commit the transaction
    if let Err(cause) = db.commit().await {
        tracing::error!(?cause, "failed to commit transaction");

        // Attempt to rollback any allocated resources in the background
        tokio::spawn(async move {
            rollback_upload_file(&search, &storage, upload_state).await;
        });

        return Err(anyhow::anyhow!("failed to commit transaction"));
    }

    Ok(output)
}

pub async fn upload_file(
    db: &mut DbTransaction<'_>,
    search: &TenantSearchIndex,
    storage: &TenantStorageLayer,
    events: &TenantEventPublisher,
    processing: &ProcessingLayer,
    upload: UploadFile,
    upload_state: &mut UploadFileState,
) -> Result<UploadedFileData, UploadFileError> {
    let s3_upload = upload.file_key.is_none();
    let file_key = upload
        .file_key
        .unwrap_or_else(|| create_file_key(&upload.document_box, &upload.name, &upload.mime));

    let mime = upload.mime;
    let file_bytes = upload.file_bytes;
    let hash = sha256::digest(file_bytes.as_ref() as &[u8]);

    // Create file to commit against
    let mut file = File::create(
        db.deref_mut(),
        CreateFile {
            parent_id: upload.parent_id,
            fixed_id: upload.fixed_id,
            name: upload.name,
            mime: mime.to_string(),
            file_key: file_key.clone(),
            folder_id: upload.folder_id,
            hash: hash.clone(),
            size: file_bytes.len().min(i32::MAX as usize) as i32,
            created_by: upload.created_by.clone(),
        },
    )
    .await
    .map_err(UploadFileError::CreateFile)?;

    // Process the file
    let processing_output = process_file(
        &upload.processing_config,
        processing,
        file_bytes.clone(),
        &mime,
    )
    .await?;

    let mut index_metadata: Option<ProcessingIndexMetadata> = None;
    let mut generated_files: Option<Vec<GeneratedFile>> = None;
    let mut additional_files: Vec<UploadedFileData> = Vec::new();

    if let Some(processing_output) = processing_output {
        // Store the encryption state for encrypted files
        if processing_output.encrypted {
            tracing::debug!("marking file as encrypted");
            file = file
                .set_encrypted(db.deref_mut(), true)
                .await
                .map_err(UploadFileError::SetEncryption)?;
        }

        index_metadata = processing_output.index_metadata;

        tracing::debug!("uploading generated files");
        let files = store_generated_files(
            db,
            storage,
            &file,
            &mut upload_state.s3_upload_keys,
            processing_output.upload_queue,
        )
        .await?;
        generated_files = Some(files);

        // Process additional files
        for additional_file in processing_output.additional_files {
            let upload = UploadFile {
                parent_id: Some(file.id),
                fixed_id: additional_file.fixed_id,
                folder_id: upload.folder_id,
                document_box: upload.document_box.clone(),
                name: additional_file.name,
                mime: additional_file.mime,
                file_bytes: additional_file.bytes,
                created_by: upload.created_by.clone(),
                file_key: None,
                processing_config: upload.processing_config.clone(),
            };

            // Process the child file (Additional file outputs are ignored)
            let output = Box::pin(upload_file(
                db,
                search,
                storage,
                events,
                processing,
                upload,
                upload_state,
            ))
            .await?;

            additional_files.push(output);
        }
    }

    // Index the file in the search index
    tracing::debug!("indexing file contents");
    store_file_index(search, &file, &upload.document_box, index_metadata).await?;
    upload_state.search_index_files.push(file.id);

    if s3_upload {
        // Upload the file itself to S3
        tracing::debug!("uploading main file");
        storage
            .upload_file(&file_key, mime.to_string(), file_bytes)
            .await
            .map_err(UploadFileError::UploadFile)?;
        upload_state.s3_upload_keys.push(file_key.clone());
    }

    // Publish an event
    events.publish_event(TenantEventMessage::FileCreated(WithScope::new(
        file.clone(),
        upload.document_box.clone(),
    )));

    Ok(UploadedFileData {
        file,
        generated: generated_files.unwrap_or_default(),
        additional_files,
    })
}

/// Stores the provided queued file uploads as generated files in
/// the database for a specific file, returns the generated file
/// database entries
///
/// Any uploads that succeed to S3 will have their file key pushed
/// to `s3_upload_keys` so that it can be rolled back if any errors
/// occur
pub async fn store_generated_files(
    db: &mut DbTransaction<'_>,
    storage: &TenantStorageLayer,
    file: &File,
    s3_upload_keys: &mut Vec<String>,
    queued_uploads: Vec<QueuedUpload>,
) -> Result<Vec<GeneratedFile>, UploadFileError> {
    // Upload the generated files to S3
    let upload_results = upload_generated_files(
        storage,
        &file.file_key,
        &file.id,
        &file.hash,
        queued_uploads,
    )
    .await;

    let mut generated_files = Vec::new();
    let mut upload_errors = Vec::new();

    for result in upload_results {
        match result {
            // Successful upload, store generated file
            Ok(create) => {
                // Track uploaded file keys
                s3_upload_keys.push(file.file_key.clone());

                // Store generated file in database
                let generated_file = match GeneratedFile::create(db.deref_mut(), create)
                    .await
                    .map_err(UploadFileError::CreateGeneratedFile)
                {
                    Ok(value) => value,
                    Err(err) => {
                        upload_errors.push(err);
                        continue;
                    }
                };

                generated_files.push(generated_file);
            }
            // Failed upload
            Err(cause) => {
                tracing::error!(?cause, "failed to upload generated file");
                upload_errors.push(UploadFileError::UploadGeneratedFile(cause));
            }
        }
    }

    // Handle any errors in the upload process
    // (This must occur after so that we can ensure we capture all successful uploads to rollback)
    if !upload_errors.is_empty() {
        tracing::warn!("error while uploading generated files, operation failed");

        let error = upload_errors
            .into_iter()
            .map(anyhow::Error::from)
            .collect::<CompositeError>();

        return Err(UploadFileError::Composite(error));
    }

    Ok(generated_files)
}

/// Performs the process of rolling back a file upload based
/// on the current upload state
pub async fn rollback_upload_file(
    search: &TenantSearchIndex,
    storage: &TenantStorageLayer,
    upload_state: UploadFileState,
) {
    // Revert upload S3 files
    for key in upload_state.s3_upload_keys {
        if let Err(err) = storage.delete_file(&key).await {
            tracing::error!(?err, "failed to rollback created tenant s3 file");
        }
    }

    // Revert file index data
    for index in upload_state.search_index_files {
        if let Err(err) = search.delete_data(index).await {
            tracing::error!(?index, ?err, "failed to rollback created file search index",);
        }
    }
}
