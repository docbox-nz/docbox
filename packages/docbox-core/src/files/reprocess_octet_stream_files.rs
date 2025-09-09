use crate::{
    files::{index_file::store_file_index, upload_file::store_generated_files},
    processing::{ProcessingIndexMetadata, ProcessingLayer, process_file},
    utils::file::get_file_name_ext,
};
use anyhow::Context;
use docbox_database::{DbPool, models::file::FileWithScope};
use docbox_search::TenantSearchIndex;
use docbox_storage::TenantStorageLayer;
use futures::{StreamExt, future::BoxFuture};
use mime::Mime;
use std::ops::DerefMut;
use tracing::Instrument;

pub async fn reprocess_octet_stream_files(
    db: &DbPool,
    search: &TenantSearchIndex,
    storage: &TenantStorageLayer,
    processing: &ProcessingLayer,
) -> anyhow::Result<()> {
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

pub async fn get_files(db: &DbPool) -> anyhow::Result<Vec<FileWithScope>> {
    let mut page_index = 0;
    let mut data = Vec::new();

    loop {
        let mut files = docbox_database::models::file::File::all_by_mime(
            db,
            "application/octet-stream",
            page_index * DATABASE_PAGE_SIZE,
            DATABASE_PAGE_SIZE,
        )
        .await
        .with_context(|| format!("failed to load files page: {page_index}"))?;

        let is_end = (files.len() as u64) < DATABASE_PAGE_SIZE;

        data.append(&mut files);

        if is_end {
            break;
        }

        page_index += 1;
    }

    Ok(data)
}

pub async fn perform_process_file(
    db: DbPool,
    storage: TenantStorageLayer,
    search: TenantSearchIndex,
    processing: ProcessingLayer,
    mut file: FileWithScope,
    mime: Mime,
) -> anyhow::Result<()> {
    // Start a database transaction
    let mut db = db.begin().await.map_err(|cause| {
        tracing::error!(?cause, "failed to begin transaction");
        anyhow::anyhow!("failed to begin transaction")
    })?;

    let bytes = storage
        .get_file(&file.file.file_key)
        .await?
        .collect_bytes()
        .await?;

    let processing_output = process_file(&None, &processing, bytes, &mime).await?;

    let mut index_metadata: Option<ProcessingIndexMetadata> = None;

    if let Some(processing_output) = processing_output {
        // Store the encryption state for encrypted files
        if processing_output.encrypted {
            tracing::debug!("marking file as encrypted");
            file.file = file.file.set_encrypted(db.deref_mut(), true).await?;
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

    file.file = file.file.set_mime(db.deref_mut(), mime.to_string()).await?;

    db.commit().await?;

    Ok(())
}
