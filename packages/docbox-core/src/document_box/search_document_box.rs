use docbox_database::{
    DbErr, DbPool, DbResult,
    models::{
        document_box::DocumentBoxScopeRaw,
        file::{File, FileId, FileWithExtra},
        folder::{Folder, FolderId, FolderPathSegment, FolderWithExtra},
        link::{Link, LinkId, LinkWithExtra},
    },
};
use docbox_search::{
    TenantSearchIndex,
    models::{
        AdminSearchRequest, FlattenedItemResult, SearchIndexType, SearchRequest, SearchResultData,
    },
};
use std::collections::HashMap;
use thiserror::Error;

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
    scope: DocumentBoxScopeRaw,
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
        .search_index(&[scope.clone()], request, search_folder_ids)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query search index");
            SearchDocumentBoxError::QueryIndex(cause)
        })?;

    let total_hits = results.total_hits;
    let results = resolve_search_results_same_scope(db, results.results, scope).await?;

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
    let results = resolve_search_results_mixed_scopes(db, results.results).await?;

    Ok(DocumentBoxSearchResults {
        results,
        total_hits,
    })
}

pub async fn resolve_search_results_same_scope(
    db: &DbPool,
    results: Vec<FlattenedItemResult>,
    scope: DocumentBoxScopeRaw,
) -> DbResult<Vec<ResolvedSearchResult>> {
    // Collect the IDs to lookup
    let mut file_ids = Vec::new();
    let mut folder_ids = Vec::new();
    let mut link_ids = Vec::new();
    for hit in &results {
        match hit.item_ty {
            SearchIndexType::File => file_ids.push(hit.item_id),
            SearchIndexType::Folder => folder_ids.push(hit.item_id),
            SearchIndexType::Link => link_ids.push(hit.item_id),
        }
    }

    // Resolve the results from the database
    let files = File::resolve_with_extra(db, &scope, file_ids);
    let folders = Folder::resolve_with_extra(db, &scope, folder_ids);
    let links = Link::resolve_with_extra(db, &scope, link_ids);
    let (files, folders, links) = tokio::try_join!(files, folders, links)?;

    // Create maps to take the results from
    let mut files: HashMap<FileId, (FileWithExtra, Vec<FolderPathSegment>)> = files
        .into_iter()
        .map(|item| (item.data.id, (item.data, item.full_path)))
        .collect();

    let mut folders: HashMap<FolderId, (FolderWithExtra, Vec<FolderPathSegment>)> = folders
        .into_iter()
        .map(|item| (item.data.id, (item.data, item.full_path)))
        .collect();

    let mut links: HashMap<LinkId, (LinkWithExtra, Vec<FolderPathSegment>)> = links
        .into_iter()
        .map(|item| (item.data.id, (item.data, item.full_path)))
        .collect();

    Ok(results
        .into_iter()
        .filter_map(|result| {
            let (result, data, path) = match result.item_ty {
                SearchIndexType::File => {
                    let file = files.remove(&result.item_id);
                    file.map(|(file, full_path)| (result, SearchResultData::File(file), full_path))
                }
                SearchIndexType::Folder => {
                    let folder = folders.remove(&result.item_id);
                    folder.map(|(folder, full_path)| {
                        (result, SearchResultData::Folder(folder), full_path)
                    })
                }
                SearchIndexType::Link => {
                    let link = links.remove(&result.item_id);
                    link.map(|(link, full_path)| (result, SearchResultData::Link(link), full_path))
                }
            }?;

            Some(ResolvedSearchResult { result, data, path })
        })
        .collect())
}

pub async fn resolve_search_results_mixed_scopes(
    db: &DbPool,
    results: Vec<FlattenedItemResult>,
) -> DbResult<Vec<ResolvedSearchResult>> {
    // Collect the IDs to lookup
    let mut file_ids = Vec::new();
    let mut folder_ids = Vec::new();
    let mut link_ids = Vec::new();

    for hit in &results {
        match hit.item_ty {
            SearchIndexType::File => file_ids.push((hit.document_box.clone(), hit.item_id)),
            SearchIndexType::Folder => folder_ids.push((hit.document_box.clone(), hit.item_id)),
            SearchIndexType::Link => link_ids.push((hit.document_box.clone(), hit.item_id)),
        }
    }

    // Resolve the results from the database
    let files = File::resolve_with_extra_mixed_scopes(db, file_ids);
    let folders = Folder::resolve_with_extra_mixed_scopes(db, folder_ids);
    let links = Link::resolve_with_extra_mixed_scopes(db, link_ids);
    let (files, folders, links) = tokio::try_join!(files, folders, links)?;

    // Create maps to take the results from
    let mut files: HashMap<(DocumentBoxScopeRaw, FileId), (FileWithExtra, Vec<FolderPathSegment>)> =
        files
            .into_iter()
            .map(|item| {
                (
                    (item.document_box, item.data.id),
                    (item.data, item.full_path),
                )
            })
            .collect();

    let mut folders: HashMap<
        (DocumentBoxScopeRaw, FolderId),
        (FolderWithExtra, Vec<FolderPathSegment>),
    > = folders
        .into_iter()
        .map(|item| {
            (
                (item.document_box, item.data.id),
                (item.data, item.full_path),
            )
        })
        .collect();

    let mut links: HashMap<(DocumentBoxScopeRaw, LinkId), (LinkWithExtra, Vec<FolderPathSegment>)> =
        links
            .into_iter()
            .map(|item| {
                (
                    (item.document_box, item.data.id),
                    (item.data, item.full_path),
                )
            })
            .collect();

    Ok(results
        .into_iter()
        .filter_map(|result| {
            let (result, data, path) = match result.item_ty {
                SearchIndexType::File => {
                    let file = files.remove(&(result.document_box.clone(), result.item_id));
                    file.map(|(file, full_path)| (result, SearchResultData::File(file), full_path))
                }
                SearchIndexType::Folder => {
                    let folder = folders.remove(&(result.document_box.clone(), result.item_id));
                    folder.map(|(folder, full_path)| {
                        (result, SearchResultData::Folder(folder), full_path)
                    })
                }
                SearchIndexType::Link => {
                    let link = links.remove(&(result.document_box.clone(), result.item_id));
                    link.map(|(link, full_path)| (result, SearchResultData::Link(link), full_path))
                }
            }?;

            Some(ResolvedSearchResult { result, data, path })
        })
        .collect())
}
