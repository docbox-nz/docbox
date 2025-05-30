use docbox_database::{
    models::{document_box::DocumentBoxScope, file::FileId},
    DbPool,
};

use crate::{
    document_box::search_document_box::{
        resolve_search_results, DocumentBoxSearchResults, SearchDocumentBoxError,
    },
    search::{models::FileSearchRequest, TenantSearchIndex},
};

pub async fn search_file(
    db: &DbPool,
    search: &TenantSearchIndex,
    scope: DocumentBoxScope,
    file_id: FileId,
    request: FileSearchRequest,
) -> Result<DocumentBoxSearchResults, SearchDocumentBoxError> {
    // Query search engine
    let results = search
        .search_index_file(&scope, file_id, request)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query search index");
            SearchDocumentBoxError::QueryIndex(cause)
        })?;

    // Resolve search results on the database end
    let total_hits = results.total_hits;
    let results = resolve_search_results(db, results.results).await;

    Ok(DocumentBoxSearchResults {
        results,
        total_hits,
    })
}
