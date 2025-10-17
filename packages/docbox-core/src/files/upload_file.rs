use crate::{
    events::{TenantEventMessage, TenantEventPublisher},
    files::{
        create_file_key,
        generated::{make_create_generated_files, upload_generated_files},
        index_file::store_file_index,
    },
};
use bytes::Bytes;
use chrono::Utc;
use docbox_database::models::{
    document_box::DocumentBoxScopeRaw, generated_file::CreateGeneratedFile,
};
use docbox_database::models::{document_box::DocumentBoxScopeRawRef, folder::FolderId};
use docbox_database::{
    DbErr, DbPool, DbTransaction,
    models::{
        document_box::WithScope,
        file::{CreateFile, File, FileId},
        generated_file::GeneratedFile,
        user::UserId,
    },
};
use docbox_processing::{
    ProcessingConfig, ProcessingError, ProcessingIndexMetadata, ProcessingLayer, QueuedUpload,
    process_file,
};
use docbox_search::{SearchError, TenantSearchIndex};
use docbox_storage::{StorageLayerError, TenantStorageLayer};
use mime::Mime;
use std::ops::DerefMut;
use thiserror::Error;
use tracing::Instrument;
use uuid::Uuid;

/// Error messages from this are user-facing so any data included should ensure
/// it does not expose any information that it should't
#[derive(Debug, Error)]
pub enum UploadFileError {
    /// Failed to create the search index
    #[error("failed to create file search index: {0}")]
    CreateIndex(SearchError),

    /// Failed to create the file database row
    #[error("failed to create file entry")]
    CreateFile(DbErr),

    /// Failed to process the file
    #[error("failed to process file: {0}")]
    Processing(#[from] ProcessingError),

    /// Failed to upload generated file to storage layer
    #[error("failed to upload generated file to storage layer: {0}")]
    UploadGeneratedFile(StorageLayerError),

    /// Failed to create the generated file database row
    #[error("failed to create generated file")]
    CreateGeneratedFile(DbErr),

    /// Failed to upload file to storage layer
    #[error("failed to upload file to storage layer: {0}")]
    UploadFile(StorageLayerError),

    /// File uploads failed
    #[error("failed to upload files")]
    FailedFileUploads(Vec<UploadFileError>),

    /// Failed to begin the transaction
    #[error("failed to perform operation (start)")]
    BeginTransaction(DbErr),

    /// Failed to commit the transaction
    #[error("failed to perform operation (end)")]
    CommitTransaction(DbErr),
}

/// State keeping track of whats been generated from a file
/// upload, to help with reverted changes on failure
#[derive(Default)]
pub struct UploadFileState {
    /// Storage file upload keys
    pub storage_upload_keys: Vec<String>,
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
    pub document_box: DocumentBoxScopeRaw,

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

pub struct UploadedFileData {
    /// The uploaded file itself
    pub file: File,
    /// Generated data alongside the file
    pub generated: Vec<GeneratedFile>,
    /// Additional files created and uploaded from processing the file
    pub additional_files: Vec<UploadedFileData>,
}

pub async fn upload_file(
    db: &DbPool,
    search: &TenantSearchIndex,
    storage: &TenantStorageLayer,
    processing: &ProcessingLayer,
    events: &TenantEventPublisher,
    upload: UploadFile,
) -> Result<UploadedFileData, UploadFileError> {
    let document_box = upload.document_box.clone();
    let mut upload_state = UploadFileState::default();

    // Perform the creation of resources and processing
    let data = match upload_file_inner(search, storage, processing, upload, &mut upload_state).await
    {
        Ok(value) => value,
        Err(error) => {
            tracing::error!(?error, "failed to complete inner file processing");
            background_rollback_upload_file(search.clone(), storage.clone(), upload_state);
            return Err(error);
        }
    };

    // Persist records to the database
    let mut db = db.begin().await.map_err(|error| {
        tracing::error!(?error, "failed to begin transaction");
        UploadFileError::BeginTransaction(error)
    })?;

    let output = match persist_file_upload(&mut db, data).await {
        Ok(value) => value,
        Err(error) => {
            if let Err(cause) = db.rollback().await {
                tracing::error!(?cause, "failed to roll back database transaction");
            }

            tracing::error!(?error, "failed to complete inner file processing");
            background_rollback_upload_file(search.clone(), storage.clone(), upload_state);
            return Err(error);
        }
    };

    if let Err(error) = db.commit().await {
        tracing::error!(?error, "failed to commit transaction");
        background_rollback_upload_file(search.clone(), storage.clone(), upload_state);
        return Err(UploadFileError::CommitTransaction(error));
    }

    // Publish creation events
    publish_file_creation_events(events, &document_box, &output);

    Ok(output)
}

/// Publish file creation events for all created files
pub fn publish_file_creation_events(
    events: &TenantEventPublisher,
    document_box: DocumentBoxScopeRawRef<'_>,
    output: &UploadedFileData,
) {
    // Publish file creation events
    events.publish_event(TenantEventMessage::FileCreated(WithScope::new(
        output.file.clone(),
        document_box.to_string(),
    )));

    for additional_file in &output.additional_files {
        publish_file_creation_events(events, document_box, additional_file);
    }
}

pub struct PreparedUploadData {
    /// Main file record to create
    file: CreateFile,

    /// Generated file records to create
    generated_files: Option<Vec<CreateGeneratedFile>>,

    /// Child additional file records to create
    additional_files: Vec<PreparedUploadData>,
}

/// Performs the file uploading, processing and storage. Prepares the data without
/// persisting data to the database
///
/// Performs the following:
/// - Process the file
/// - Create a prepared file record database metadata
/// - Upload generated files and create their metadata
/// - Perform this function for additional inner files
/// - Store file metadata in the search index
/// - Upload the main file to S3 if not already performed
async fn upload_file_inner(
    search: &TenantSearchIndex,
    storage: &TenantStorageLayer,
    processing: &ProcessingLayer,
    upload: UploadFile,
    upload_state: &mut UploadFileState,
) -> Result<PreparedUploadData, UploadFileError> {
    // Determine if we need to upload and what the file key is
    let (s3_upload, file_key) = match upload.file_key.as_ref() {
        // Already have a file key, don't want to upload
        Some(file_key) => (false, file_key.clone()),

        // No existing file key, we are creating one and uploading the file
        None => (
            true,
            create_file_key(
                &upload.document_box,
                &upload.name,
                &upload.mime,
                Uuid::new_v4(),
            ),
        ),
    };

    // Process the file
    let processing_output = process_file(
        &upload.processing_config,
        processing,
        upload.file_bytes.clone(),
        &upload.mime,
    )
    .await?;

    // Get file encryption state
    let encrypted = processing_output
        .as_ref()
        .map(|output| output.encrypted)
        .unwrap_or_default();

    let file_record = make_file_record(&upload, &file_key, &upload.file_bytes, encrypted);

    let mut index_metadata: Option<ProcessingIndexMetadata> = None;
    let mut generated_files: Option<Vec<CreateGeneratedFile>> = None;
    let mut additional_files: Vec<PreparedUploadData> = Vec::new();

    if let Some(processing_output) = processing_output {
        index_metadata = processing_output.index_metadata;

        // Upload generated files and store the metadata
        tracing::debug!("uploading generated files");
        let prepared_files = store_generated_files(
            storage,
            &file_record,
            &mut upload_state.storage_upload_keys,
            processing_output.upload_queue,
        )
        .await?;
        generated_files = Some(prepared_files);

        // Process additional files
        for additional_file in processing_output.additional_files {
            let upload = UploadFile {
                parent_id: Some(file_record.id),
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
            let output = Box::pin(upload_file_inner(
                search,
                storage,
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
    store_file_index(search, &file_record, &upload.document_box, index_metadata).await?;
    upload_state.search_index_files.push(file_record.id);

    if s3_upload {
        // Upload the file itself to S3
        tracing::debug!("uploading main file");
        storage
            .upload_file(&file_key, file_record.mime.clone(), upload.file_bytes)
            .await
            .map_err(UploadFileError::UploadFile)?;
        upload_state.storage_upload_keys.push(file_key.clone());
    }

    Ok(PreparedUploadData {
        file: file_record,
        generated_files,
        additional_files,
    })
}

/// Persists the data from [PreparedUploadData] into the database storing any applied changes
async fn persist_file_upload(
    db: &mut DbTransaction<'_>,
    data: PreparedUploadData,
) -> Result<UploadedFileData, UploadFileError> {
    // Create file to commit against
    let file = File::create(db.deref_mut(), data.file)
        .await
        .map_err(UploadFileError::CreateFile)?;

    // Create generated file records
    let mut generated_files = Vec::new();
    if let Some(creates) = data.generated_files {
        for create in creates {
            let generated_file = GeneratedFile::create(db.deref_mut(), create)
                .await
                .map_err(UploadFileError::CreateGeneratedFile)?;

            generated_files.push(generated_file);
        }
    }

    // Create records for inner additional files
    let mut additional_files: Vec<UploadedFileData> = Vec::new();
    for additional_file in data.additional_files {
        let inner = Box::pin(persist_file_upload(db, additional_file)).await?;
        additional_files.push(inner);
    }

    Ok(UploadedFileData {
        file,
        generated: generated_files,
        additional_files,
    })
}

/// Creates a file record to be stored in the database
fn make_file_record(
    upload: &UploadFile,
    file_key: &str,
    file_bytes: &Bytes,
    encrypted: bool,
) -> CreateFile {
    let id = upload.fixed_id.unwrap_or_else(Uuid::new_v4);
    let hash = sha256::digest(file_bytes.as_ref() as &[u8]);
    let size = file_bytes.len().min(i32::MAX as usize) as i32;
    let created_at = Utc::now();

    CreateFile {
        id,
        parent_id: upload.parent_id,
        name: upload.name.clone(),
        mime: upload.mime.to_string(),
        file_key: file_key.to_owned(),
        folder_id: upload.folder_id,
        hash,
        size,
        created_by: upload.created_by.clone(),
        created_at,
        encrypted,
    }
}

/// Creates prepared upload records for generated files, stores the files
/// in S3 and returns the [CreateGeneratedFile] structures to be stored
/// in the database at a later step
///
/// Any uploads that succeed to storage will have their file key pushed
/// to `storage_upload_keys` so that it can be rolled back if any errors
/// occur
pub async fn store_generated_files(
    storage: &TenantStorageLayer,
    file: &CreateFile,
    storage_upload_keys: &mut Vec<String>,
    queued_uploads: Vec<QueuedUpload>,
) -> Result<Vec<CreateGeneratedFile>, UploadFileError> {
    let prepared_uploads =
        make_create_generated_files(&file.file_key, &file.id, &file.hash, queued_uploads);

    // Upload the generated files to S3
    let upload_results = upload_generated_files(storage, prepared_uploads).await;

    let mut generated_files = Vec::new();
    let mut upload_errors = Vec::new();

    for result in upload_results {
        match result {
            // Successful upload, store generated file
            Ok(create) => {
                // Track uploaded file keys
                storage_upload_keys.push(file.file_key.clone());
                generated_files.push(create);
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
        tracing::warn!(
            ?upload_errors,
            "error while uploading generated files, operation failed"
        );

        return Err(UploadFileError::FailedFileUploads(upload_errors));
    }

    Ok(generated_files)
}

/// Performs a background rollback task on an uploaded file
fn background_rollback_upload_file(
    search: TenantSearchIndex,
    storage: TenantStorageLayer,
    upload_state: UploadFileState,
) {
    let span = tracing::Span::current();

    // Attempt to rollback any allocated resources in the background
    tokio::spawn(
        async move {
            rollback_upload_file(&search, &storage, upload_state).await;
        }
        .instrument(span),
    );
}

/// Performs the process of rolling back a file upload based
/// on the current upload state
async fn rollback_upload_file(
    search: &TenantSearchIndex,
    storage: &TenantStorageLayer,
    upload_state: UploadFileState,
) {
    // Revert upload S3 files
    for key in upload_state.storage_upload_keys {
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
