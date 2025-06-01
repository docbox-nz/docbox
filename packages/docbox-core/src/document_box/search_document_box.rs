use anyhow::Context;
use docbox_database::{
    models::{
        document_box::DocumentBoxScope,
        file::File,
        folder::{Folder, FolderPathSegment},
        link::Link,
    },
    DbErr, DbPool,
};
use futures::StreamExt;
use thiserror::Error;

use crate::search::{
    models::{
        AdminSearchRequest, FlattenedItemResult, SearchIndexType, SearchRequest, SearchResultData,
    },
    TenantSearchIndex,
};

#[derive(Debug, Error)]
pub enum SearchDocumentBoxError {
    #[error(transparent)]
    Database(#[from] DbErr),

    #[error("document box missing root folder")]
    MissingRoot,

    #[error(transparent)]
    QueryIndex(anyhow::Error),
}

pub struct ResolvedSearchResult {
    /// Result from opensearch
    pub result: FlattenedItemResult,

    /// Resolve result from database
    pub data: SearchResultData,

    /// Path to the item
    pub path: Vec<FolderPathSegment>,
}

pub struct DocumentBoxSearchResults {
    pub results: Vec<ResolvedSearchResult>,
    pub total_hits: u64,
}

pub async fn search_document_box(
    db: &DbPool,
    search: &TenantSearchIndex,
    scope: DocumentBoxScope,
    request: SearchRequest,
) -> Result<DocumentBoxSearchResults, SearchDocumentBoxError> {
    // When searching within a specific folder resolve all allowed folder ID's
    let search_folder_ids = match request.folder_id {
        Some(folder_id) => {
            let folder = Folder::find_by_id(db, &scope, folder_id)
                .await
                .inspect_err(|error| tracing::error!(?error, "failed to query root folder"))?
                .ok_or_else(|| {
                    tracing::error!("failed to find folder");
                    SearchDocumentBoxError::MissingRoot
                })?;

            let folder_children = folder
                .tree_all_children(db)
                .await
                .inspect_err(|error| tracing::error!(?error, "failed to query folder children"))?;

            Some(folder_children)
        }
        None => None,
    };

    tracing::debug!(?search_folder_ids, "searching within folders");

    // Query search engine
    let results = search
        .search_index(&[scope], request, search_folder_ids)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query search index");
            SearchDocumentBoxError::QueryIndex(cause)
        })?;

    let total_hits = results.total_hits;
    let results = resolve_search_results(db, results.results).await;

    Ok(DocumentBoxSearchResults {
        results,
        total_hits,
    })
}

pub async fn search_document_boxes_admin(
    db: &DbPool,
    search: &TenantSearchIndex,
    request: AdminSearchRequest,
) -> Result<DocumentBoxSearchResults, SearchDocumentBoxError> {
    // Query search engine
    let results = search
        .search_index(&request.scopes, request.request, None)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query search index");
            SearchDocumentBoxError::QueryIndex(cause)
        })?;

    let total_hits = results.total_hits;
    let results = resolve_search_results(db, results.results).await;

    Ok(DocumentBoxSearchResults {
        results,
        total_hits,
    })
}

pub async fn resolve_search_results(
    db: &DbPool,
    results: Vec<FlattenedItemResult>,
) -> Vec<ResolvedSearchResult> {
    let chunk_results: Vec<Option<ResolvedSearchResult>> = futures::stream::iter(results)
        .map(|result| async move {
            match resolve_search_result(db, result).await {
                Ok((result, data, path)) => Some(ResolvedSearchResult { result, data, path }),
                Err(cause) => {
                    tracing::error!("failed to fetch search result from database {cause:?}");
                    None
                }
            }
        })
        // Process at most 20 resolves at a time
        .buffered(20)
        .collect()
        .await;

    chunk_results.into_iter().flatten().collect()
}

async fn resolve_search_result(
    db: &DbPool,
    hit: FlattenedItemResult,
) -> anyhow::Result<(
    FlattenedItemResult,
    SearchResultData,
    Vec<FolderPathSegment>,
)> {
    let (data, path) = match hit.item_ty {
        SearchIndexType::File => {
            let file = File::find_with_extra(db, &hit.document_box, hit.item_id)
                .await
                .context("failed to query file")?
                .context("file present in search results doesn't exist")?;
            let path = File::resolve_path(db, hit.item_id).await?;

            (SearchResultData::File(file), path)
        }
        SearchIndexType::Folder => {
            let folder = Folder::find_by_id_with_extra(db, &hit.document_box, hit.item_id)
                .await
                .context("failed to query folder")?
                .context("folder present in search results doesn't exist")?;
            let path = Folder::resolve_path(db, hit.item_id).await?;

            (SearchResultData::Folder(folder), path)
        }
        SearchIndexType::Link => {
            let link = Link::find_with_extra(db, &hit.document_box, hit.item_id)
                .await
                .context("failed to query link")?
                .context("link present in search results doesn't exist")?;
            let path = Link::resolve_path(db, hit.item_id).await?;

            (SearchResultData::Link(link), path)
        }
    };

    Ok((hit, data, path))
}
