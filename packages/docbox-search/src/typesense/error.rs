use thiserror::Error;

#[derive(Debug, Error)]
pub enum TypesenseIndexFactoryError {
    #[error("missing TYPESENSE_URL env")]
    MissingUrl,

    #[error("must provide either api_key or api_key_secret_name for search config")]
    MissingApiKey,

    #[error("failed to create http client")]
    CreateClient,
}

#[derive(Debug, Error)]
pub enum TypesenseSearchError {
    #[error("missing search result")]
    MissingSearchResult,
    #[error("must provide either include_name or include_content")]
    MissingQueryBy,
    #[error("failed to create index")]
    CreateIndex,
    #[error("failed to get index")]
    GetIndex,
    #[error("failed to delete index")]
    DeleteIndex,
    #[error("failed to get secret")]
    GetSecret,
    #[error("failed to bulk add documents")]
    BulkAddDocuments,
    #[error("failed to delete search documents")]
    DeleteDocuments,
    #[error("failed to update document")]
    UpdateDocument,
    #[error("failed to get document")]
    GetDocument,
    #[error("missing root entry to update")]
    MissingRootEntry,
    #[error("failed to search index")]
    SearchIndex,
}
