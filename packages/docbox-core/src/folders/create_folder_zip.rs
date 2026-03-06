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
    models::folder::{Folder, FolderId},
};
use docbox_storage::{StorageLayer, StorageLayerError, UploadFileTag};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::task::{JoinError, spawn_blocking};
use utoipa::ToSchema;
use uuid::Uuid;
use zip::{ZipWriter, result::ZipError, write::SimpleFileOptions};

use crate::{
    files::create_file_key,
    folders::folder_stream::{FolderWalkError, FolderWalkItem, FolderWalkStream},
};

#[derive(Debug, Error)]
pub enum CreateFolderZipError {
    #[error("failed to files within target folder")]
    GetFiles(DbErr),

    #[error("failed to walk folder when creating zip")]
    WalkFolder(#[from] FolderWalkError),

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

    #[error(transparent)]
    InvalidFilePathError(#[from] InvalidFilePathError),
}

/// Options when creating a ZIP file from a folder
#[derive(Debug, Clone)]
pub struct CreateFolderZipOptions {
    /// Optionally only include the specified files
    pub include: Option<Vec<Uuid>>,
    /// Optionally exclude the specified files including
    /// all other files
    pub exclude: Option<Vec<Uuid>>,
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

    let mut stream = FolderWalkStream::new(db, folder.clone());

    let mut folders = HashMap::new();
    let mut files = Vec::new();

    // Walk the folder tree to obtain all the child folders and files
    while let Some(result) = stream.next().await {
        let item = result?;

        match item {
            FolderWalkItem::Folder(folder) => {
                folders.insert(folder.id, folder);
            }

            FolderWalkItem::File(file) => {
                files.push(file);
            }

            // Links are not included in exports
            FolderWalkItem::Link(_) => continue,
        }
    }

    let mut files_with_data = Vec::new();

    // Download all of the files
    for file in files {
        // Exclusion filter is applied to all files
        if options
            .exclude
            .as_ref()
            .is_some_and(|excludes| excludes.contains(&file.id))
        {
            continue;
        }

        // Inclusion filter is only applied to direct descendants and
        // not the children of other folders
        if file.folder_id == folder.id
            && options
                .include
                .as_ref()
                .is_some_and(|includes| !includes.contains(&file.id))
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
    let zip_bytes = spawn_blocking(move || -> Result<Bytes, CreateFolderZipError> {
        let mut zip = ZipWriter::new(Cursor::new(Vec::<u8>::new()));

        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        let mut seen_names_at: HashMap<Uuid, HashSet<String>> = HashMap::new();

        let folders = folders;

        for (file, bytes) in files_with_data {
            let folder_path = make_folder_path(&folders, file.folder_id)?;
            let mut name = zip_safe_name(&file.name);

            // Prefix duplicate files with the file ID which is known to be unique
            let seen_folder_names = seen_names_at.entry(file.folder_id).or_default();
            if seen_folder_names.contains(&name) {
                name = zip_safe_name(&format!("{}_{}", file.id, file.name));
            } else {
                seen_folder_names.insert(name.clone());
            }

            let entry_name = if !folder_path.is_empty() {
                format!("{folder_path}/{name}")
            } else {
                name
            };

            zip.start_file(&entry_name, options)?;
            zip.write_all(&bytes).map_err(ZipError::Io)?;
        }

        let cursor = zip.finish()?;

        Ok(Bytes::from(cursor.into_inner()))
    })
    .await
    .map_err(CreateFolderZipError::JoinTaskError)??;

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

#[derive(Debug, Error)]
#[error("file had an incomplete folder path")]
pub struct InvalidFilePathError;

/// Using a `parent_id` and a collection of `folders` resolve the folder path name
/// to the item based on its parent.
///
/// `seen_folder_names` tracks the names of folders seen within a specific parent folder
/// and is used to ensure duplicate folder paths are not created
fn make_folder_path(
    folders: &HashMap<FolderId, Folder>,
    parent_id: FolderId,
) -> Result<String, InvalidFilePathError> {
    let mut parts = Vec::new();
    let mut folder_id = parent_id;

    loop {
        let folder = folders.get(&folder_id).ok_or(InvalidFilePathError)?;
        let parent_id = match folder.folder_id {
            Some(value) => value,
            // Don't append a "Root" folder to the path, these are internal
            None => break,
        };

        let safe_name = zip_safe_name(&folder.name);
        parts.push(safe_name);
        folder_id = parent_id;
    }

    // Paths are iterated in reverse order so we need to flip them here
    parts.reverse();

    Ok(parts.join("/"))
}

/// Helper to create a zip entry safe name
fn zip_safe_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}
