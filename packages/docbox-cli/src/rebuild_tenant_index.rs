use std::str::FromStr;

use docbox_core::{
    aws::aws_config,
    office::is_pdf_compatible,
    processing::pdf::PAGE_END_CHARACTER,
    search::{
        models::{DocumentPage, SearchIndexData, SearchIndexType},
        SearchIndexFactory,
    },
    secrets::AppSecretManager,
    storage::{StorageLayerFactory, TenantStorageLayer},
};
use docbox_database::{
    models::{
        document_box::DocumentBoxScopeRaw,
        file::{File, FileWithScope},
        folder::Folder,
        generated_file::{GeneratedFile, GeneratedFileType},
        link::{Link, LinkWithScope},
        tenant::Tenant,
    },
    DatabasePoolCache, DbPool,
};
use eyre::{Context, ContextCompat};
use futures::{future::LocalBoxFuture, stream::FuturesUnordered, StreamExt};
use itertools::Itertools;
use uuid::Uuid;

use crate::{AnyhowError, CliConfiguration};

pub async fn rebuild_tenant_index(
    config: &CliConfiguration,
    env: String,
    tenant_id: Uuid,
) -> eyre::Result<()> {
    tracing::debug!(?env, ?tenant_id, "rebuilding tenant index");

    // Load AWS configuration
    let aws_config = aws_config().await;

    // Connect to secrets manager
    let secrets = AppSecretManager::from_config(&aws_config, config.secrets.clone());

    tracing::info!("created database secret");

    // Setup database cache / connector
    let db_cache = DatabasePoolCache::new(
        config.database.host.clone(),
        config.database.port,
        // In the CLI the db credentials have high enough access to be used as the
        // "root secret"
        "postgres/docbox/config".to_string(),
        secrets,
    );

    let search_factory = SearchIndexFactory::from_config(&aws_config, config.search.clone())
        .map_err(|err| eyre::Error::msg(err.to_string()))?;

    // Setup S3 access
    let storage_factory = StorageLayerFactory::from_config(&aws_config, config.storage.clone());

    let root_db = db_cache.get_root_pool().await?;
    let tenant = Tenant::find_by_id(&root_db, tenant_id, &env)
        .await?
        .context("tenant not found")?;

    let db = db_cache.get_tenant_pool(&tenant).await?;
    let search = search_factory.create_search_index(&tenant);
    let storage = storage_factory.create_storage_layer(&tenant);

    tracing::info!(?tenant, "started re-indexing tenant");

    _ = search.create_index().await;

    let links = create_links_index_data(&db).await?;
    let folders = create_folders_index_data(&db).await?;
    let files = create_files_index_data(&db, &storage).await?;

    let index_data = links
        .into_iter()
        .chain(folders.into_iter())
        .chain(files.into_iter())
        .collect::<Vec<SearchIndexData>>();

    tracing::debug!("all data loaded: {}", index_data.len());

    {
        let serialized = serde_json::to_string(&index_data).unwrap();
        tokio::fs::write("index_data.json", serialized)
            .await
            .unwrap();
    }

    let index_data_chunks = index_data.into_iter().chunks(INDEX_CHUNK_SIZE);
    let index_data_chunks = index_data_chunks.into_iter();

    for data in index_data_chunks {
        let chunk = data.collect::<Vec<_>>();
        search.bulk_add_data(chunk).await.unwrap();
    }

    Ok(())
}

const INDEX_CHUNK_SIZE: usize = 5000;
/// Size of each page to request from the database
const DATABASE_PAGE_SIZE: u64 = 1000;
/// Number of files to process in parallel
const FILE_PROCESS_SIZE: usize = 500;

/// Collects all stored links and creates the [SearchIndexData] for them
pub async fn create_links_index_data(db: &DbPool) -> eyre::Result<Vec<SearchIndexData>> {
    let mut page_index = 0;
    let mut data = Vec::new();

    loop {
        let links = Link::all(db, page_index * DATABASE_PAGE_SIZE, DATABASE_PAGE_SIZE)
            .await
            .with_context(|| format!("failed to load files page: {page_index}"))?;
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
pub async fn create_folders_index_data(db: &DbPool) -> eyre::Result<Vec<SearchIndexData>> {
    let mut page_index = 0;
    let mut data = Vec::new();

    loop {
        let folders = Folder::all_non_root(db, page_index * DATABASE_PAGE_SIZE, DATABASE_PAGE_SIZE)
            .await
            .with_context(|| format!("failed to load folders page: {page_index}"))?;
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
) -> eyre::Result<Vec<SearchIndexData>> {
    let mut page_index = 0;
    let mut data = Vec::new();
    let mut files_for_processing = Vec::new();

    loop {
        let files = File::all(db, page_index * DATABASE_PAGE_SIZE, DATABASE_PAGE_SIZE)
            .await
            .with_context(|| format!("failed to load files page: {page_index}"))?;

        let is_end = (files.len() as u64) < DATABASE_PAGE_SIZE;

        for FileWithScope { file, scope } in files {
            let mime = mime::Mime::from_str(&file.mime).context("invalid file mime type")?;

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

/// Attempts to obtain the [DocumentPage] collection for a PDF compatible file
pub async fn try_pdf_compatible_document_pages(
    db: &DbPool,
    storage: &TenantStorageLayer,
    scope: &DocumentBoxScopeRaw,
    file: &File,
) -> eyre::Result<Vec<DocumentPage>> {
    // Load the extracted text content for the file
    let text_file = GeneratedFile::find(db, scope, file.id, GeneratedFileType::TextContent)
        .await?
        .context("missing text content")?;

    tracing::debug!(?text_file, "loaded file generated text content");

    // Read the PDF file from S3
    let text_content = storage
        .get_file(&text_file.file_key)
        .await
        .map_err(AnyhowError)?
        .collect_bytes()
        .await
        .inspect_err(|cause| {
            tracing::error!(?cause, "failed to load pdf bytes from s3 stream");
        })
        .map_err(AnyhowError)?;

    // Load the text content
    let text_content = text_content.to_vec();
    let text_content = String::from_utf8(text_content).context("invalid utf8 text content")?;

    // Split the content back into pages
    let pages = text_content.split(PAGE_END_CHARACTER);

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
