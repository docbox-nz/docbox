use crate::{
    files::{
        index_file::store_file_index,
        upload_file::{UploadFileError, store_generated_files},
    },
    processing::{ProcessingError, ProcessingIndexMetadata, ProcessingLayer, process_file},
    utils::file::get_file_name_ext,
};
use docbox_database::{DbPool, DbResult, models::file::FileWithScope};
use docbox_search::TenantSearchIndex;
use docbox_storage::{StorageLayerError, TenantStorageLayer};
use futures::{StreamExt, future::BoxFuture};
use mime::Mime;
use std::ops::DerefMut;
use thiserror::Error;
use tracing::Instrument;

pub async fn reprocess_octet_stream_files(
    db: &DbPool,
    search: &TenantSearchIndex,
    storage: &TenantStorageLayer,
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
}

pub async fn perform_process_file(
    db: DbPool,
    storage: TenantStorageLayer,
    search: TenantSearchIndex,
    processing: ProcessingLayer,
    mut file: FileWithScope,
    mime: Mime,
) -> Result<(), ProcessFileError> {
    // Start a database transaction
    let mut db = db.begin().await.map_err(|cause| {
        tracing::error!(?cause, "failed to begin transaction");
        ProcessFileError::BeginTransaction
    })?;

    let bytes = storage
        .get_file(&file.file.file_key)
        .await
        .inspect_err(|error| tracing::error!(?error, "Failed to get storage file"))?
        .collect_bytes()
        .await
        .inspect_err(|error| tracing::error!(?error, "Failed to get storage file"))?;

    let processing_output = process_file(&None, &processing, bytes, &mime).await?;

    let mut index_metadata: Option<ProcessingIndexMetadata> = None;

    if let Some(processing_output) = processing_output {
        // Store the encryption state for encrypted files
        if processing_output.encrypted {
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

        index_metadata = processing_output.index_metadata;

        let mut s3_upload_keys = Vec::new();

        tracing::debug!("uploading generated files");
        store_generated_files(
            &mut db,
            &storage,
            &file.file,
            &mut s3_upload_keys,
            processing_output.upload_queue,
        )
        .await?;
    }

    // Index the file in the search index
    tracing::debug!("indexing file contents");
    store_file_index(&search, &file.file, &file.scope, index_metadata).await?;

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
