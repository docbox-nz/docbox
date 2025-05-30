use docbox_database::{
    models::{
        document_box::DocumentBoxScope,
        folder::{Folder, FolderPathSegment},
    },
    DbErr, DbPool,
};
use futures::StreamExt;
use thiserror::Error;

use crate::search::{
    models::{AdminSearchRequest, FlattenedItemResult, SearchRequest, SearchResultData},
    os::resolve_search_result,
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
