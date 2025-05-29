//! Business logic for working with generated files

use anyhow::Context;
use bytes::Bytes;
use mime::Mime;
use tracing::{debug, error};
use uuid::Uuid;

use crate::{storage::TenantStorageLayer, utils::file::get_mime_ext};
use futures::{
    stream::{FuturesOrdered, FuturesUnordered},
    StreamExt,
};

use docbox_database::models::{
    file::FileId,
    generated_file::{CreateGeneratedFile, GeneratedFile, GeneratedFileId, GeneratedFileType},
};

#[derive(Debug)]
pub struct QueuedUpload {
    mime: Mime,
    ty: GeneratedFileType,
    bytes: Bytes,
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
    Err(Vec<GeneratedFileId>, anyhow::Error),
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

                debug!(%id, %file_id, %file_key, "uploading file to s3",);

                // Delete file from S3
                if let Err(cause) = storage.delete_file(&file_key).await {
                    error!(%id, %file_id, %file_key, ?cause, "failed to delete generated file");
                }

                debug!("deleted file from s3");

                anyhow::Ok(id)
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
) -> Vec<anyhow::Result<CreateGeneratedFile>> {
    queued_uploads
        .into_iter()
        .map(|queued_upload| {
            // Task needs its own copies of state
            let file_id = *file_id;
            let file_hash = file_hash.to_string();

            async move {
                let file_mime = queued_upload.mime.to_string();

                // Mapped file extensions for
                let file_ext = get_mime_ext(&queued_upload.mime).unwrap_or("bin");

                // Generate a unique file key
                let file_key = Uuid::new_v4().to_string();
                // Prefix the file key with the document box scope and a "generated" suffix
                let file_key = format!("{}_{}.generated.{}", base_file_key, file_key, file_ext);

                debug!(%file_id, %file_hash, %file_key, %file_mime, "uploading file to s3");

                // Upload the file to S3
                storage
                    .upload_file(&file_key, file_mime, queued_upload.bytes)
                    .await
                    .context("failed to upload generated file")?;

                anyhow::Ok(CreateGeneratedFile {
                    file_id,
                    hash: file_hash,
                    mime: queued_upload.mime.to_string(),
                    ty: queued_upload.ty,
                    file_key,
                })
            }
        })
        .collect::<FuturesOrdered<_>>()
        .collect()
        .await
}
