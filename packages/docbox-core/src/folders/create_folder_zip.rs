use std::{
    collections::{HashMap, HashSet},
    io::{Cursor, Write},
    str::FromStr,
    time::Duration,
};

use bytes::Bytes;
use chrono::{DateTime, Utc};
use docbox_database::{
    DbErr, DbPool,
    models::{
        file::{File, FileId},
        folder::Folder,
    },
};
use docbox_storage::{StorageLayer, StorageLayerError, UploadFileTag};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::task::{JoinError, spawn_blocking};
use utoipa::ToSchema;
use uuid::Uuid;
use zip::{ZipWriter, result::ZipError, write::SimpleFileOptions};

use crate::files::create_file_key;

#[derive(Debug, Error)]
pub enum CreateFolderZipError {
    #[error("failed to files within target folder")]
    GetFiles(DbErr),

    #[error("failed to create zip")]
    Zip(#[from] ZipError),

    #[error("failed to upload zip file")]
    UploadZip(StorageLayerError),

    #[error("failed to create to zip download")]
    CreateZipDownload(StorageLayerError),

    #[error("failed to start file download")]
    DownloadFile(StorageLayerError),

    #[error("failed to join zip task")]
    JoinTaskError(JoinError),
}

/// Options when creating a ZIP file from a folder
#[derive(Debug, Clone)]
pub struct CreateFolderZipOptions {
    /// Optionally only include the specified files
    pub include_files: Option<Vec<FileId>>,
    /// Optionally exclude the specified files including
    /// all other files
    pub exclude_files: Option<Vec<FileId>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateFolderZipDownloadResponse {
    pub method: String,
    pub uri: String,
    pub headers: HashMap<String, String>,
    pub expires_at: DateTime<Utc>,
}

/// Create a compressed ZIP file from the provided folder contents
pub async fn create_folder_zip(
    db: &DbPool,
    storage: &StorageLayer,
    folder: &Folder,
    options: CreateFolderZipOptions,
) -> Result<CreateFolderZipDownloadResponse, CreateFolderZipError> {
    let bundle_id = Uuid::new_v4();
    let bundle_name = "bundle.zip";

    let mime = mime::Mime::from_str("application/zip").expect("zip mime should always be valid");
    let file_key = create_file_key(&folder.document_box, bundle_name, &mime, bundle_id);

    let files = File::find_by_parent(db, folder.id)
        .await
        .map_err(CreateFolderZipError::GetFiles)?;

    let mut files_with_data = Vec::new();

    // download all of the files
    for file in files {
        // Apply filters
        if options
            .include_files
            .as_ref()
            .is_some_and(|includes| !includes.contains(&file.id))
            || options
                .exclude_files
                .as_ref()
                .is_some_and(|excludes| excludes.contains(&file.id))
        {
            continue;
        }

        let bytes = storage
            .get_file(&file.file_key)
            .await
            .map_err(CreateFolderZipError::DownloadFile)?
            .collect_bytes()
            .await
            .map_err(CreateFolderZipError::DownloadFile)?;
        files_with_data.push((file, bytes));
    }

    // Move the zip compression to a blocking task to prevent slowing down the
    // async runtime
    let zip_bytes = spawn_blocking(move || -> Result<Bytes, ZipError> {
        let mut zip = ZipWriter::new(Cursor::new(Vec::<u8>::new()));

        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        let mut seen_names = HashSet::<String>::new();

        for (file, bytes) in files_with_data {
            let mut name = file.name;

            // Prefix duplicate files with the file ID which is known to be unique
            if seen_names.contains(&name) {
                name = format!("{}_{}", file.id, name);
            }

            zip.start_file(&name, options)?;
            zip.write_all(&bytes)?;
            seen_names.insert(name);
        }

        let cursor = zip.finish()?;

        Ok(Bytes::from(cursor.into_inner()))
    })
    .await
    .map_err(CreateFolderZipError::JoinTaskError)?
    .map_err(CreateFolderZipError::Zip)?;

    storage
        .upload_file(
            &file_key,
            zip_bytes,
            docbox_storage::UploadFileOptions {
                content_type: "application/zip".to_string(),
                tags: Some(vec![UploadFileTag::ExpireDays1]),
            },
        )
        .await
        .inspect_err(|error| tracing::error!(?error, "failed to upload zip file"))
        .map_err(CreateFolderZipError::UploadZip)?;

    let (signed_request, expires_at) = storage
        .create_presigned_download(&file_key, Duration::from_secs(60 * 60 * 24))
        .await
        .inspect_err(|error| tracing::error!(?error, "failed to create zip presigned download"))
        .map_err(CreateFolderZipError::CreateZipDownload)?;

    Ok(CreateFolderZipDownloadResponse {
        method: signed_request.method().to_string(),
        uri: signed_request.uri().to_string(),
        headers: signed_request
            .headers()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect(),
        expires_at,
    })
}
