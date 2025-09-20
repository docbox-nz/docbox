use thiserror::Error;

#[derive(Debug, Error)]
pub enum OpenSearchIndexFactoryError {
    #[error("missing OPENSEARCH_URL env")]
    MissingUrl,
    #[error("failed to parse opensearch url")]
    InvalidUrl,
    #[error("failed to create opensearch auth config")]
    CreateAuthConfig,
    #[error("failed to build search transport")]
    BuildTransport,
}

#[derive(Debug, Error)]
pub enum OpenSearchSearchError {
    #[error("failed to create index")]
    CreateIndex,
    #[error("failed to delete index")]
    DeleteIndex,
    #[error("failed to search index")]
    SearchIndex,
    #[error("failed to add search data")]
    AddData,
    #[error("failed to update search data")]
    UpdateData,
    #[error("failed to delete search data")]
    DeleteData,
}
