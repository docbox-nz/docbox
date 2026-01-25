//! # Reprocess application/octet-stream
//!
//! This is a migration helper script, used to handle the case where file
//! types were not known at the time of ingest and were taken in as simply
//! application/octet-stream files.
//!
//! This migration takes all of the files matching that mime type and attempts
//! to infer the file mime type based on its extension and perform the processing
//! step to generate its processed variants and update the file mime type

use crate::{
    files::{
        index_file::store_file_index,
        upload_file::{UploadFileError, store_generated_files},
    },
    utils::{file::get_file_name_ext, timing::handle_slow_future},
};
use docbox_database::{
    DbPool, DbResult,
    models::{
        file::{CreateFile, FileWithScope},
        generated_file::{CreateGeneratedFile, GeneratedFile},
    },
};
use docbox_processing::{
    DEFAULT_PROCESS_TIMEOUT, ProcessingError, ProcessingIndexMetadata, ProcessingLayer,
    process_file,
};
use docbox_search::TenantSearchIndex;
use docbox_storage::{StorageLayer, StorageLayerError};
use futures::{StreamExt, future::BoxFuture};
use mime::Mime;
use std::{ops::DerefMut, time::Duration};
use thiserror::Error;
use tokio::time::timeout;
use tracing::Instrument;

pub async fn reprocess_octet_stream_files(
    db: &DbPool,
    search: &TenantSearchIndex,
    storage: &StorageLayer,
    processing: &ProcessingLayer,
) -> DbResult<()> {
    _ = search.create_index().await;

    let files = get_files(db).await?;
    let mut skipped = Vec::new();
    let mut processing_files = Vec::new();

    for file in files {
        let guessed_mime = get_file_name_ext(&file.file.name).and_then(|ext| {
            let guesses = mime_guess::from_ext(&ext);
            guesses.first()
        });

        if let Some(mime) = guessed_mime {
            processing_files.push((file, mime));
        } else {
            skipped.push(file);
        }
    }

    let span = tracing::Span::current();

    // Process all the files
    _ = futures::stream::iter(processing_files)
        .map(|(file, mime)| -> BoxFuture<'static, ()> {
            let db = db.clone();
            let search = search.clone();
            let storage = storage.clone();
            let processing = processing.clone();
            let span = span.clone();

            Box::pin(
                async move {
                    tracing::debug!(?file, "stating file");
                    if let Err(error) =
                        perform_process_file(db, storage, search, processing, file, mime).await
                    {
                        tracing::error!(?error, "failed to migrate file");
                    };
                }
                .instrument(span),
            )
        })
        .buffered(FILE_PROCESS_SIZE)
        .collect::<Vec<()>>()
        .await;

    for skipped in skipped {
        tracing::debug!(file_id = %skipped.file.id, file_name = %skipped.file.name, "skipped file");
    }

    Ok(())
}

/// Size of each page to request from the database
const DATABASE_PAGE_SIZE: u64 = 1000;
/// Number of files to process in parallel
const FILE_PROCESS_SIZE: usize = 50;

pub async fn get_files(db: &DbPool) -> DbResult<Vec<FileWithScope>> {
    let mut page_index = 0;
    let mut data = Vec::new();

    loop {
        let mut files = match docbox_database::models::file::File::all_by_mime(
            db,
            "application/octet-stream",
            page_index * DATABASE_PAGE_SIZE,
            DATABASE_PAGE_SIZE,
        )
        .await
        {
            Ok(value) => value,
            Err(error) => {
                tracing::error!(?error, ?page_index, "failed to load files page");
                return Err(error);
            }
        };

        let is_end = (files.len() as u64) < DATABASE_PAGE_SIZE;

        data.append(&mut files);

        if is_end {
            break;
        }

        page_index += 1;
    }

    Ok(data)
}

#[derive(Debug, Error)]
pub enum ProcessFileError {
    #[error("failed to begin transaction")]
    BeginTransaction,

    #[error("failed to commit transaction")]
    CommitTransaction,

    #[error(transparent)]
    Storage(#[from] StorageLayerError),

    #[error(transparent)]
    Process(#[from] ProcessingError),

    #[error(transparent)]
    UploadFile(#[from] UploadFileError),

    #[error("failed to mark file as encrypted")]
    SetEncrypted,

    #[error("failed to update file mime")]
    SetMime,

    #[error("timeout occurred while processing file")]
    ConvertTimeout,
}

/// TODO: Handle rollback for failure
pub async fn perform_process_file(
    db: DbPool,
    storage: StorageLayer,
    search: TenantSearchIndex,
    processing: ProcessingLayer,
    mut file: FileWithScope,
    mime: Mime,
) -> Result<(), ProcessFileError> {
    let bytes = storage
        .get_file(&file.file.file_key)
        .await
        .inspect_err(|error| tracing::error!(?error, "Failed to get storage file"))?
        .collect_bytes()
        .await
        .inspect_err(|error| tracing::error!(?error, "Failed to get storage file"))?;

    let process_future = process_file(&None, &processing, bytes, &mime);

    let process_timeout = processing
        .config
        .process_timeout
        .unwrap_or(DEFAULT_PROCESS_TIMEOUT);

    // Apply a 120s timeout to file processing, we can assume it has definitely failed if its taken that long
    let process_future = timeout(process_timeout, process_future);

    // Apply a slow future warning to the processing future
    let processing_output = handle_slow_future(process_future, Duration::from_secs(25), || {
        tracing::warn!(
            ?file,
            "file upload processing has taken over 25s to complete"
        )
    })
    .await
    .map_err(|_| ProcessFileError::ConvertTimeout)??;

    let mut index_metadata: Option<ProcessingIndexMetadata> = None;

    let file_in = &file.file;

    let created_file = CreateFile {
        id: file_in.id,
        parent_id: file_in.parent_id,
        name: file_in.name.clone(),
        mime: file_in.mime.to_string(),
        file_key: file_in.file_key.to_owned(),
        folder_id: file_in.folder_id,
        hash: file_in.hash.clone(),
        size: file_in.size,
        created_by: file_in.created_by.clone(),
        created_at: file_in.created_at,
        encrypted: file_in.encrypted,
    };

    let mut generated_files: Option<Vec<CreateGeneratedFile>> = None;

    // Get file encryption state
    let encrypted = processing_output
        .as_ref()
        .map(|output| output.encrypted)
        .unwrap_or_default();

    if let Some(processing_output) = processing_output {
        index_metadata = processing_output.index_metadata;

        let mut s3_upload_keys = Vec::new();

        tracing::debug!("uploading generated files");
        let prepared_files = store_generated_files(
            &storage,
            &created_file,
            &mut s3_upload_keys,
            processing_output.upload_queue,
        )
        .await?;
        generated_files = Some(prepared_files);
    }

    // Index the file in the search index
    tracing::debug!("indexing file contents");
    store_file_index(&search, &created_file, &file.scope, index_metadata).await?;

    // Start a database transaction
    let mut db = db.begin().await.map_err(|error| {
        tracing::error!(?error, "failed to begin transaction");
        ProcessFileError::BeginTransaction
    })?;

    // Create generated file records
    if let Some(creates) = generated_files {
        for create in creates {
            GeneratedFile::create(db.deref_mut(), create)
                .await
                .map_err(UploadFileError::CreateGeneratedFile)?;
        }
    }

    if encrypted {
        // Mark the file as encrypted
        tracing::debug!("marking file as encrypted");
        file.file = file
            .file
            .set_encrypted(db.deref_mut(), true)
            .await
            .map_err(|error| {
                tracing::error!(?error, "failed to set file as encrypted");
                ProcessFileError::SetEncrypted
            })?;
    }

    // Update the file mime type
    tracing::debug!("updating file mime type");
    file.file = file
        .file
        .set_mime(db.deref_mut(), mime.to_string())
        .await
        .map_err(|error| {
            tracing::error!(?error, "failed to set file mime");
            ProcessFileError::SetMime
        })?;

    db.commit().await.map_err(|error| {
        tracing::error!(?error, "failed to commit transaction");
        ProcessFileError::CommitTransaction
    })?;

    Ok(())
}
