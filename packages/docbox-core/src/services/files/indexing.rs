use anyhow::Context;
use futures::TryFutureExt;
use pdf_process::{
    pdf_info, text::PAGE_END_CHARACTER, text_all_pages_split, PdfInfoArgs, PdfTextArgs,
};
use std::str::FromStr;

use crate::{
    processing::{ProcessingError, ProcessingIndexMetadata},
    search::{
        models::{DocumentPage, SearchIndexData, SearchIndexType, UpdateSearchIndexData},
        TenantSearchIndex,
    },
    services::{
        generated::QueuedUpload,
        pdf::{is_pdf_compatible, is_pdf_file},
    },
    storage::TenantStorageLayer,
};

use docbox_database::{
    models::{
        document_box::DocumentBoxScope,
        file::File,
        generated_file::{GeneratedFile, GeneratedFileType},
    },
    DbPool,
};

use super::upload::{store_generated_files, UploadFileError};

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
        created_at: file.created_at.to_rfc3339(),
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

#[allow(unused)]
pub async fn re_index_file(
    db: &DbPool,
    search: &TenantSearchIndex,
    storage: &TenantStorageLayer,
    scope: &DocumentBoxScope,
    file: File,
    fresh: bool,
) -> anyhow::Result<()> {
    // No text processing for encrypted files
    if file.encrypted {
        if fresh {
            // Re-create base file index
            store_file_index(search, &file, scope, None).await?;
        }

        return Ok(());
    }

    let mime = mime::Mime::from_str(&file.mime).context("invalid file mime type")?;

    let pdf_file_bytes = if is_pdf_file(&mime) {
        // Read the PDF file from S3
        match storage
            .get_file(&file.file_key)
            .await?
            .collect_bytes()
            .await
        {
            Ok(value) => value,
            Err(cause) => {
                tracing::error!(?cause, ?file, "failed to load pdf bytes from s3 stream");
                anyhow::bail!("failed to load pdf bytes from s3 stream");
            }
        }
    } else if is_pdf_compatible(&mime) {
        // Load the generated pdf file and text file
        let pdf_file = GeneratedFile::find(db, scope, file.id, GeneratedFileType::Pdf).await?;

        tracing::debug!(?pdf_file, "loaded file generated pdf");

        // No PDF file to process for text content
        let pdf_file = match pdf_file {
            Some(value) => value,
            None => return Ok(()),
        };

        // Read the PDF file from S3
        match storage
            .get_file(&pdf_file.file_key)
            .await?
            .collect_bytes()
            .await
        {
            Ok(value) => value,
            Err(cause) => {
                tracing::error!(?cause, ?pdf_file, "failed to load pdf bytes from s3 stream");
                anyhow::bail!("failed to load pdf bytes from s3 stream");
            }
        }
    } else {
        if fresh {
            // Re-create base file index
            store_file_index(search, &file, scope, None).await?;
        }

        return Ok(());
    };

    // Load the generated text
    let text_file = GeneratedFile::find(db, scope, file.id, GeneratedFileType::TextContent).await?;

    let pdf_info_args = PdfInfoArgs::default();

    // Load the pdf information
    let pdf_info = match pdf_info(&pdf_file_bytes, &pdf_info_args).await {
        Ok(value) => value,
        Err(cause) => {
            tracing::error!(?cause, "failed to get pdf file info");
            anyhow::bail!("failed to get pdf file info");
        }
    };

    // Get available pages
    let page_count = pdf_info
        .pages()
        .ok_or(ProcessingError::MalformedFile)?
        .map_err(|_| ProcessingError::MalformedFile)?;

    // No content to index
    if page_count < 1 {
        if fresh {
            // Re-create base file index
            store_file_index(search, &file, scope, None).await?;
        }

        return Ok(());
    }

    let text_args = PdfTextArgs::default();

    // Extract pdf text
    let pages = match text_all_pages_split(&pdf_file_bytes, &text_args)
        // Match outer result type with inner type
        .map_err(ProcessingError::ExtractFileText)
        .await
    {
        Ok(value) => value,
        Err(cause) => {
            tracing::error!(?cause, "failed to get pdf pages text");
            anyhow::bail!("failed to get pdf pages text");
        }
    };

    // Create a combined text content using the PDF page end character
    let page_end = PAGE_END_CHARACTER.to_string();
    let combined_text_content = pages.join(&page_end).as_bytes().to_vec();

    let pages = pages
        .into_iter()
        .enumerate()
        .map(|(page, content)| DocumentPage {
            page: page as u64,
            content,
        })
        .collect();

    if fresh {
        // Re-create base file index
        store_file_index(
            search,
            &file,
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
                    folder_id: None,
                    name: None,
                    mime: None,
                    content: None,
                    created_at: None,
                    created_by: None,
                    document_box: None,
                    pages: Some(pages),
                },
            )
            .await
            .map_err(UploadFileError::CreateIndex)?;
    }

    let queued_uploads = vec![QueuedUpload::new(
        mime::TEXT_PLAIN,
        GeneratedFileType::TextContent,
        combined_text_content.into(),
    )];

    {
        let mut db = db.begin().await?;
        let mut upload_keys = Vec::new();
        store_generated_files(&mut db, storage, &file, &mut upload_keys, queued_uploads).await?;
        db.commit().await?;
    }

    // Delete the associated text file (The old version)
    if let Some(text_file) = text_file {
        if let Err(cause) = storage.delete_file(&text_file.file_key).await {
            tracing::error!(
                ?cause,
                ?text_file,
                "failed to delete previous generated text file from s3"
            );
            anyhow::bail!("failed to delete previous generated text file from s3")
        }

        if let Err(cause) = text_file.delete(db).await {
            tracing::error!(?cause, "failed to delete previous text file from db");
            anyhow::bail!("failed to delete previous text file from db")
        }
    }

    Ok(())
}
