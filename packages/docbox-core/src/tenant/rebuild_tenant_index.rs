use docbox_database::{
    DbErr, DbPool, DbResult,
    models::{
        document_box::DocumentBoxScopeRaw,
        file::{File, FileWithScope},
        folder::Folder,
        generated_file::{GeneratedFile, GeneratedFileType},
        link::{Link, LinkWithScope},
    },
};
use docbox_processing::{office::is_pdf_compatible, pdf::split_pdf_text_pages};
use docbox_search::{
    SearchError, TenantSearchIndex,
    models::{DocumentPage, SearchIndexData, SearchIndexType},
};
use docbox_storage::{StorageLayerError, TenantStorageLayer};
use futures::{StreamExt, future::LocalBoxFuture, stream::FuturesUnordered};
use itertools::Itertools;
use std::{str::FromStr, string::FromUtf8Error};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RebuildTenantIndexError {
    #[error(transparent)]
    Database(#[from] DbErr),
    #[error(transparent)]
    Search(#[from] SearchError),
    #[error(transparent)]
    WriteIndexData(std::io::Error),
    #[error(transparent)]
    SerializeIndexData(serde_json::Error),
}

/// Rebuild the search index for the tenant based on that
/// data stored in the database and the content stored in S3
pub async fn rebuild_tenant_index(
    db: &DbPool,
    search: &TenantSearchIndex,
    storage: &TenantStorageLayer,
) -> Result<(), RebuildTenantIndexError> {
    tracing::info!("started re-indexing tenant");

    let index_data = recreate_search_index_data(db, storage).await?;
    tracing::debug!("all data loaded: {}", index_data.len());

    {
        let serialized = serde_json::to_string(&index_data)
            .map_err(RebuildTenantIndexError::SerializeIndexData)?;
        tokio::fs::write("index_data.json", serialized)
            .await
            .map_err(RebuildTenantIndexError::WriteIndexData)?;
    }

    apply_rebuilt_tenant_index(search, index_data).await?;

    Ok(())
}

/// Apply the rebuilt tenant index
pub async fn apply_rebuilt_tenant_index(
    search: &TenantSearchIndex,
    data: Vec<SearchIndexData>,
) -> Result<(), SearchError> {
    // Ensure the index exists
    _ = search.create_index().await;

    let index_data_chunks = data.into_iter().chunks(INDEX_CHUNK_SIZE);
    let index_data_chunks = index_data_chunks.into_iter();

    for data in index_data_chunks {
        let chunk = data.collect::<Vec<_>>();
        search.add_data(chunk).await?;
    }

    Ok(())
}

/// Rebuild the entire tenant search index
pub async fn recreate_search_index_data(
    db: &DbPool,
    storage: &TenantStorageLayer,
) -> DbResult<Vec<SearchIndexData>> {
    let links = create_links_index_data(db).await?;
    let folders = create_folders_index_data(db).await?;
    let files = create_files_index_data(db, storage).await?;

    let index_data = links
        .into_iter()
        .chain(folders.into_iter())
        .chain(files.into_iter())
        .collect::<Vec<SearchIndexData>>();

    Ok(index_data)
}

const INDEX_CHUNK_SIZE: usize = 5000;
/// Size of each page to request from the database
const DATABASE_PAGE_SIZE: u64 = 1000;
/// Number of files to process in parallel
const FILE_PROCESS_SIZE: usize = 500;

/// Collects all stored links and creates the [SearchIndexData] for them
pub async fn create_links_index_data(db: &DbPool) -> DbResult<Vec<SearchIndexData>> {
    let mut page_index = 0;
    let mut data = Vec::new();

    loop {
        let links = match Link::all(db, page_index * DATABASE_PAGE_SIZE, DATABASE_PAGE_SIZE).await {
            Ok(value) => value,
            Err(error) => {
                tracing::error!(?error, ?page_index, "failed to load links page");
                return Err(error);
            }
        };
        let is_end = (links.len() as u64) < DATABASE_PAGE_SIZE;

        for LinkWithScope { link, scope } in links {
            data.push(SearchIndexData {
                ty: SearchIndexType::Link,
                item_id: link.id,
                folder_id: link.folder_id,
                name: link.name.to_string(),
                mime: None,
                content: Some(link.value.clone()),
                pages: None,
                created_at: link.created_at,
                created_by: link.created_by.clone(),
                document_box: scope.clone(),
            })
        }

        if is_end {
            break;
        }

        page_index += 1;
    }

    Ok(data)
}

/// Collects all stored non-root folders and creates the [SearchIndexData] for them
pub async fn create_folders_index_data(db: &DbPool) -> DbResult<Vec<SearchIndexData>> {
    let mut page_index = 0;
    let mut data = Vec::new();

    loop {
        let folders =
            match Folder::all_non_root(db, page_index * DATABASE_PAGE_SIZE, DATABASE_PAGE_SIZE)
                .await
            {
                Ok(value) => value,
                Err(error) => {
                    tracing::error!(?error, ?page_index, "failed to load folders page");
                    return Err(error);
                }
            };
        let is_end = (folders.len() as u64) < DATABASE_PAGE_SIZE;

        for folder in folders {
            let folder_id = match folder.folder_id {
                Some(value) => value,
                // Root folders are not included in the index
                None => continue,
            };

            data.push(SearchIndexData {
                ty: SearchIndexType::Folder,
                item_id: folder.id,
                folder_id,
                name: folder.name.to_string(),
                mime: None,
                content: None,
                pages: None,
                created_at: folder.created_at,
                created_by: folder.created_by.clone(),
                document_box: folder.document_box.clone(),
            })
        }

        if is_end {
            break;
        }

        page_index += 1;
    }

    Ok(data)
}

pub async fn create_files_index_data(
    db: &DbPool,
    storage: &TenantStorageLayer,
) -> DbResult<Vec<SearchIndexData>> {
    let mut page_index = 0;
    let mut data = Vec::new();
    let mut files_for_processing = Vec::new();

    loop {
        let files = match File::all(db, page_index * DATABASE_PAGE_SIZE, DATABASE_PAGE_SIZE).await {
            Ok(value) => value,
            Err(error) => {
                tracing::error!(?error, ?page_index, "failed to load files page");
                return Err(error);
            }
        };
        let is_end = (files.len() as u64) < DATABASE_PAGE_SIZE;

        for FileWithScope { file, scope } in files {
            let mime = match mime::Mime::from_str(&file.mime) {
                Ok(value) => value,
                Err(error) => {
                    tracing::error!(?error, ?file, "file has an invalid mime type");
                    continue;
                }
            };

            if file.encrypted || !is_pdf_compatible(&mime) {
                // These files don't require any processing
                data.push(SearchIndexData {
                    ty: SearchIndexType::File,
                    item_id: file.id,
                    folder_id: file.folder_id,
                    name: file.name,
                    mime: Some(file.mime),
                    content: None,
                    created_at: file.created_at,
                    created_by: file.created_by,
                    document_box: scope,
                    pages: None,
                })
            } else {
                // File needs additional processing
                files_for_processing.push((file, scope));
            }
        }

        if is_end {
            break;
        }

        page_index += 1;
    }

    for chunk in files_for_processing.chunks(FILE_PROCESS_SIZE) {
        let mut results: Vec<SearchIndexData> = chunk
            .iter()
            .map(|(file, scope)| -> LocalBoxFuture<'_, SearchIndexData> {
                Box::pin(async move {
                    let pages =
                        match try_pdf_compatible_document_pages(db, storage, scope, file).await {
                            Ok(value) => value,
                            Err(cause) => {
                                tracing::error!(?cause, "failed to re-create pdf index data pages");
                                return SearchIndexData {
                                    ty: SearchIndexType::File,
                                    item_id: file.id,
                                    folder_id: file.folder_id,
                                    name: file.name.clone(),
                                    mime: Some(file.mime.clone()),
                                    content: None,
                                    created_at: file.created_at,
                                    created_by: file.created_by.clone(),
                                    document_box: scope.clone(),
                                    pages: None,
                                };
                            }
                        };

                    SearchIndexData {
                        ty: SearchIndexType::File,
                        item_id: file.id,
                        folder_id: file.folder_id,
                        name: file.name.clone(),
                        mime: Some(file.mime.clone()),
                        content: None,
                        created_at: file.created_at,
                        created_by: file.created_by.clone(),
                        document_box: scope.clone(),
                        pages: Some(pages),
                    }
                })
            })
            .collect::<FuturesUnordered<LocalBoxFuture<'_, SearchIndexData>>>()
            .collect()
            .await;

        data.append(&mut results);
    }

    Ok(data)
}

#[derive(Debug, Error)]
pub enum PdfCompatibleRebuildError {
    #[error("file is missing text content")]
    MissingTextContent,

    #[error(transparent)]
    Database(#[from] DbErr),

    #[error(transparent)]
    Storage(#[from] StorageLayerError),

    #[error(transparent)]
    InvalidTextContent(#[from] FromUtf8Error),
}

/// Attempts to obtain the [DocumentPage] collection for a PDF compatible file
pub async fn try_pdf_compatible_document_pages(
    db: &DbPool,
    storage: &TenantStorageLayer,
    scope: &DocumentBoxScopeRaw,
    file: &File,
) -> Result<Vec<DocumentPage>, PdfCompatibleRebuildError> {
    // Load the extracted text content for the file
    let text_file = GeneratedFile::find(db, scope, file.id, GeneratedFileType::TextContent)
        .await?
        .ok_or(PdfCompatibleRebuildError::MissingTextContent)?;

    tracing::debug!(?text_file, "loaded file generated text content");

    // Read the PDF file from S3
    let text_content = storage
        .get_file(&text_file.file_key)
        .await?
        .collect_bytes()
        .await
        .inspect_err(|cause| {
            tracing::error!(?cause, "failed to load pdf bytes from s3 stream");
        })?;

    // Load the text content
    let text_content = text_content.to_vec();
    let text_content =
        String::from_utf8(text_content).map_err(PdfCompatibleRebuildError::InvalidTextContent)?;

    // Split the content back into pages
    let pages = split_pdf_text_pages(&text_content);

    // Create the pages data
    let pages = pages
        .into_iter()
        .enumerate()
        .map(|(page, content)| DocumentPage {
            page: page as u64,
            content: content.to_string(),
        })
        .collect();

    Ok(pages)
}
