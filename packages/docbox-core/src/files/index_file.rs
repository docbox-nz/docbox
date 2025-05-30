use anyhow::Context;
use futures::{future::BoxFuture, stream::FuturesUnordered, StreamExt};
use pdf_process::text::PAGE_END_CHARACTER;
use std::str::FromStr;

use crate::{
    office::is_pdf_compatible,
    processing::ProcessingIndexMetadata,
    search::{
        models::{DocumentPage, SearchIndexData, SearchIndexType, UpdateSearchIndexData},
        TenantSearchIndex,
    },
    storage::TenantStorageLayer,
};

use docbox_database::{
    models::{
        document_box::DocumentBoxScope,
        file::File,
        generated_file::{GeneratedFile, GeneratedFileType},
        tenant::Tenant,
    },
    DbPool,
};

use super::upload_file::UploadFileError;

pub async fn store_file_index(
    search: &TenantSearchIndex,
    file: &File,
    document_box: &DocumentBoxScope,
    index_metadata: Option<ProcessingIndexMetadata>,
) -> Result<(), UploadFileError> {
    // Use index from previous step or create new index
    let index = SearchIndexData {
        ty: SearchIndexType::File,
        item_id: file.id,
        folder_id: file.folder_id,
        name: file.name.to_string(),
        mime: Some(file.mime.clone()),
        content: None,
        created_at: file.created_at,
        created_by: file.created_by.clone(),
        document_box: document_box.clone(),
        pages: index_metadata.and_then(|value| value.pages),
    };

    search
        .add_data(index)
        .await
        .map_err(UploadFileError::CreateIndex)?;

    Ok(())
}

pub async fn re_index_files(
    db: &DbPool,
    search: &TenantSearchIndex,
    storage: &TenantStorageLayer,
    tenant: &Tenant,
) -> anyhow::Result<()> {
    let tenant_id = tenant.id;

    let mut page_index = 0;
    const PAGE_SIZE: u64 = 5000;

    loop {
        let files = File::all(db, page_index * PAGE_SIZE, PAGE_SIZE)
            .await
            .with_context(|| format!("failed to load files page: {page_index}"))?;

        let is_end = (files.len() as u64) < PAGE_SIZE;

        // Apply migration to all tenants
        let results: Vec<anyhow::Result<()>> = files
            .into_iter()
            .map(|file| -> BoxFuture<'_, anyhow::Result<()>> {
                Box::pin(async move {
                    let file_id = file.file.id;

                    re_index_file(db, search, storage, &file.scope, &file.file, true)
                        .await
                        .with_context(|| {
                            format!(
                            "failed to migrate folder: Tenant ID: {tenant_id} File ID: {file_id}"
                        )
                        })
                })
            })
            .collect::<FuturesUnordered<BoxFuture<'_, anyhow::Result<()>>>>()
            .collect()
            .await;

        for result in results {
            if let Err(err) = result {
                tracing::error!(?err, "failed to migrate tenant")
            }
        }

        if is_end {
            break;
        }

        page_index += 1;
    }

    Ok(())
}

pub async fn re_index_file(
    db: &DbPool,
    search: &TenantSearchIndex,
    storage: &TenantStorageLayer,
    scope: &DocumentBoxScope,
    file: &File,
    fresh: bool,
) -> anyhow::Result<()> {
    // No text processing for encrypted files
    if file.encrypted {
        if fresh {
            store_file_index(search, file, scope, None).await?;
        }

        return Ok(());
    }

    let mime = mime::Mime::from_str(&file.mime).context("invalid file mime type")?;

    if !is_pdf_compatible(&mime) {
        if fresh {
            // Re-create base file index
            store_file_index(search, file, scope, None).await?;
        }

        return Ok(());
    }

    let pages = match try_pdf_compatible_document_pages(db, storage, scope, file).await {
        Ok(value) => value,
        Err(cause) => {
            if fresh {
                // Re-create base file index
                store_file_index(search, file, scope, None).await?;
            }

            tracing::error!(?cause, "failed to re-create pdf index data pages");
            return Ok(());
        }
    };

    if fresh {
        // Re-create base file index
        store_file_index(
            search,
            file,
            scope,
            Some(ProcessingIndexMetadata { pages: Some(pages) }),
        )
        .await?;
    } else {
        // Create the file search index
        search
            .update_data(
                file.id,
                UpdateSearchIndexData {
                    folder_id: file.folder_id,
                    name: file.name.clone(),
                    content: None,
                    pages: Some(pages),
                },
            )
            .await
            .map_err(UploadFileError::CreateIndex)?;
    }

    Ok(())
}

/// Attempts to obtain the [DocumentPage] collection for a PDF compatible file
pub async fn try_pdf_compatible_document_pages(
    db: &DbPool,
    storage: &TenantStorageLayer,
    scope: &DocumentBoxScope,
    file: &File,
) -> anyhow::Result<Vec<DocumentPage>> {
    // Load the extracted text content for the file
    let text_file = GeneratedFile::find(db, scope, file.id, GeneratedFileType::TextContent)
        .await?
        .context("missing text content")?;

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
