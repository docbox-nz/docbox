//! Business logic for working with generated files

use crate::files::create_generated_file_key;
use chrono::Utc;
use docbox_database::models::{
    file::FileId,
    generated_file::{CreateGeneratedFile, GeneratedFile, GeneratedFileId},
};
use docbox_processing::QueuedUpload;
use docbox_storage::{StorageLayer, StorageLayerError};
use futures::{
    StreamExt,
    stream::{FuturesOrdered, FuturesUnordered},
};
use tracing::{Instrument, debug, error};
use uuid::Uuid;

pub enum GeneratedFileDeleteResult {
    /// Successful upload of all files
    Ok,
    /// Error path contains any files that were upload
    /// along with the error that occurred
    Err(Vec<GeneratedFileId>, StorageLayerError),
}

pub async fn delete_generated_files(
    storage: &StorageLayer,
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

pub struct PreparedGeneratedFile {
    create: CreateGeneratedFile,
    upload: QueuedUpload,
}

pub fn make_create_generated_files(
    base_file_key: &str,
    file_id: &FileId,
    file_hash: &str,
    queued_uploads: Vec<QueuedUpload>,
) -> Vec<PreparedGeneratedFile> {
    queued_uploads
        .into_iter()
        .map(|upload| {
            let id = Uuid::new_v4();
            let created_at = Utc::now();
            let file_key = create_generated_file_key(base_file_key, &upload.mime);

            let create = CreateGeneratedFile {
                id,
                file_id: *file_id,
                hash: file_hash.to_string(),
                mime: upload.mime.to_string(),
                ty: upload.ty,
                file_key,
                created_at,
            };

            PreparedGeneratedFile { create, upload }
        })
        .collect()
}

/// Triggers the file uploads returning a list of the [CreateGeneratedFile] structures
/// to persist to the database
pub async fn upload_generated_files(
    storage: &StorageLayer,
    prepared: Vec<PreparedGeneratedFile>,
) -> Vec<Result<CreateGeneratedFile, StorageLayerError>> {
    prepared
        .into_iter()
        .map(|PreparedGeneratedFile { create, upload }| {
            let span = tracing::info_span!("upload_generated_files", ?create);
            async move {
                // Upload the file to storage
                storage
                    .upload_file(&create.file_key, create.mime.clone(), upload.bytes)
                    .await
                    .inspect_err(|error| {
                        tracing::error!(?error, "failed to store generated file");
                    })?;

                Ok(create)
            }
            .instrument(span)
        })
        .collect::<FuturesOrdered<_>>()
        .collect()
        .await
}
