//! Business logic for working with generated files

use crate::files::create_generated_file_key;
use bytes::Bytes;
use docbox_storage::{StorageLayerError, TenantStorageLayer};
use futures::{
    StreamExt,
    stream::{FuturesOrdered, FuturesUnordered},
};
use mime::Mime;
use tracing::{Instrument, debug, error};

use docbox_database::models::{
    file::FileId,
    generated_file::{CreateGeneratedFile, GeneratedFile, GeneratedFileId, GeneratedFileType},
};

#[derive(Debug)]
pub struct QueuedUpload {
    pub mime: Mime,
    pub ty: GeneratedFileType,
    pub bytes: Bytes,
}

impl QueuedUpload {
    pub fn new(mime: Mime, ty: GeneratedFileType, bytes: Bytes) -> Self {
        Self { mime, ty, bytes }
    }
}

pub enum GeneratedFileDeleteResult {
    /// Successful upload of all files
    Ok,
    /// Error path contains any files that were upload
    /// along with the error that occurred
    Err(Vec<GeneratedFileId>, StorageLayerError),
}

pub async fn delete_generated_files(
    storage: &TenantStorageLayer,
    files: &[GeneratedFile],
) -> GeneratedFileDeleteResult {
    let files_count = files.len();

    let mut futures = files
        .iter()
        .map(|file| {
            async {
                let id = file.id;
                let file_id = file.file_id;
                let file_key = file.file_key.to_string();

                debug!(%id, %file_id, %file_key, "deleting file from storage");

                // Delete file from storage
                if let Err(error) = storage.delete_file(&file_key).await {
                    error!(%id, %file_id, %file_key, ?error, "failed to delete generated file");
                    return Err(error);
                }

                debug!("deleted file from storage");
                Ok(id)
            }
        })
        .collect::<FuturesUnordered<_>>();

    let mut deleted: Vec<GeneratedFileId> = Vec::with_capacity(files_count);

    while let Some(result) = futures.next().await {
        match result {
            Ok(id) => deleted.push(id),
            Err(err) => return GeneratedFileDeleteResult::Err(deleted, err),
        }
    }

    GeneratedFileDeleteResult::Ok
}

/// Triggers the file uploads returning a list of the [CreateGeneratedFile] structures
/// to persist to the database
pub async fn upload_generated_files(
    storage: &TenantStorageLayer,
    base_file_key: &str,
    file_id: &FileId,
    file_hash: &str,
    queued_uploads: Vec<QueuedUpload>,
) -> Vec<Result<CreateGeneratedFile, StorageLayerError>> {
    queued_uploads
        .into_iter()
        .map(|queued_upload| {
            // Task needs its own copies of state
            let file_id = *file_id;
            let file_hash = file_hash.to_string();
            let file_mime = queued_upload.mime.to_string();
            let file_key = create_generated_file_key(base_file_key, &queued_upload.mime);
            let span = tracing::info_span!("upload_generated_files", %file_id, %file_hash, %file_key, %file_mime);

            async move {
                // Upload the file to storage
                storage
                    .upload_file(&file_key, file_mime, queued_upload.bytes)
                    .await
                    .inspect_err(|error |{
                        tracing::error!(?error, "failed to store generated file");
                    })?;


                Ok(CreateGeneratedFile {
                    file_id,
                    hash: file_hash,
                    mime: queued_upload.mime.to_string(),
                    ty: queued_upload.ty,
                    file_key,
                })
            }
            .instrument(span)
        })
        .collect::<FuturesOrdered<_>>()
        .collect()
        .await
}
