use aws_config::SdkConfig;
use docbox_database::models::{
    document_box::DocumentBoxScopeRaw, file::FileId, folder::FolderId, tenant::Tenant,
};
use models::{
    FileSearchRequest, FileSearchResults, SearchIndexData, SearchRequest, SearchResults,
    UpdateSearchIndexData,
};
use serde::Deserialize;
use uuid::Uuid;

pub mod models;
mod serialize;

#[cfg(feature = "opensearch")]
pub use opensearch::OpenSearchConfig;
#[cfg(feature = "typesense")]
pub use typesense::TypesenseSearchConfig;

#[cfg(feature = "opensearch")]
mod opensearch;
#[cfg(feature = "typesense")]
mod typesense;

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum SearchIndexFactoryConfig {
    #[cfg(feature = "typesense")]
    Typesense(typesense::TypesenseSearchConfig),
    #[cfg(feature = "opensearch")]
    OpenSearch(opensearch::OpenSearchConfig),
}

impl SearchIndexFactoryConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let variant = std::env::var("DOCBOX_SEARCH_INDEX_FACTORY")
            .unwrap_or_else(|_| "typesense".to_string());
        match variant.as_str() {
            #[cfg(feature = "opensearch")]
            "open_search" => opensearch::OpenSearchConfig::from_env().map(Self::OpenSearch),
            #[cfg(feature = "typesense")]
            "typesense" => typesense::TypesenseSearchConfig::from_env().map(Self::Typesense),

            // Default when typesense is enabled
            #[cfg(feature = "typesense")]
            _ => typesense::TypesenseSearchConfig::from_env().map(Self::Typesense),
            // Default when typesense is disabled
            #[cfg(all(not(feature = "typesense"), feature = "opensearch"))]
            _ => opensearch::OpenSearchConfig::from_env().map(Self::OpenSearch),

            // Fallback error when no features are available
            #[cfg(not(any(feature = "typesense", feature = "opensearch")))]
            _ => panic!("no matching search index factory is available"),
        }
    }
}

#[derive(Clone)]
pub enum SearchIndexFactory {
    #[cfg(feature = "typesense")]
    Typesense(typesense::TypesenseIndexFactory),
    #[cfg(feature = "opensearch")]
    OpenSearch(opensearch::OpenSearchIndexFactory),
}

impl SearchIndexFactory {
    pub fn from_config(
        aws_config: &SdkConfig,
        config: SearchIndexFactoryConfig,
    ) -> anyhow::Result<Self> {
        #[cfg(not(feature = "opensearch"))]
        let _ = aws_config;

        match config {
            #[cfg(feature = "typesense")]
            SearchIndexFactoryConfig::Typesense(config) => {
                tracing::debug!("using typesense search index");
                typesense::TypesenseIndexFactory::from_config(config)
                    .map(SearchIndexFactory::Typesense)
            }
            #[cfg(feature = "opensearch")]
            SearchIndexFactoryConfig::OpenSearch(config) => {
                tracing::debug!("using opensearch search index");
                opensearch::OpenSearchIndexFactory::from_config(aws_config, config)
                    .map(SearchIndexFactory::OpenSearch)
            }
        }
    }

    /// Create a new "OpenSearch" search index for the tenant
    pub fn create_search_index(&self, tenant: &Tenant) -> TenantSearchIndex {
        match self {
            #[cfg(feature = "typesense")]
            SearchIndexFactory::Typesense(factory) => {
                let search_index = tenant.os_index_name.clone();
                TenantSearchIndex::Typesense(factory.create_search_index(search_index))
            }
            #[cfg(feature = "opensearch")]
            SearchIndexFactory::OpenSearch(factory) => {
                let search_index = opensearch::TenantSearchIndexName::from_tenant(tenant);
                TenantSearchIndex::OpenSearch(factory.create_search_index(search_index))
            }

            // Fallback error when no features are available
            #[cfg(not(any(feature = "typesense", feature = "opensearch")))]
            _ => panic!("no matching search index is available"),
        }
    }
}

pub enum TenantSearchIndex {
    #[cfg(feature = "typesense")]
    Typesense(typesense::TypesenseIndex),
    #[cfg(feature = "opensearch")]
    OpenSearch(opensearch::OpenSearchIndex),
}

impl TenantSearchIndex {
    pub async fn create_index(&self) -> anyhow::Result<()> {
        match self {
            #[cfg(feature = "typesense")]
            TenantSearchIndex::Typesense(index) => index.create_index().await,
            #[cfg(feature = "opensearch")]
            TenantSearchIndex::OpenSearch(index) => index.create_index().await,

            // Fallback error when no features are available
            #[cfg(not(any(feature = "typesense", feature = "opensearch")))]
            _ => panic!("no matching search index is available"),
        }
    }

    pub async fn delete_index(&self) -> anyhow::Result<()> {
        match self {
            #[cfg(feature = "typesense")]
            TenantSearchIndex::Typesense(index) => index.delete_index().await,
            #[cfg(feature = "opensearch")]
            TenantSearchIndex::OpenSearch(index) => index.delete_index().await,

            // Fallback error when no features are available
            #[cfg(not(any(feature = "typesense", feature = "opensearch")))]
            _ => panic!("no matching search index is available"),
        }
    }

    pub async fn search_index(
        &self,
        scope: &[DocumentBoxScopeRaw],
        query: SearchRequest,
        folder_children: Option<Vec<FolderId>>,
    ) -> anyhow::Result<SearchResults> {
        match self {
            #[cfg(feature = "typesense")]
            TenantSearchIndex::Typesense(index) => {
                index.search_index(scope, query, folder_children).await
            }
            #[cfg(feature = "opensearch")]
            TenantSearchIndex::OpenSearch(index) => {
                index.search_index(scope, query, folder_children).await
            }

            // Fallback error when no features are available
            #[cfg(not(any(feature = "typesense", feature = "opensearch")))]
            _ => panic!("no matching search index is available"),
        }
    }

    /// Searches the index for matches scoped to a specific file
    pub async fn search_index_file(
        &self,
        scope: &DocumentBoxScopeRaw,
        file_id: FileId,
        query: FileSearchRequest,
    ) -> anyhow::Result<FileSearchResults> {
        match self {
            #[cfg(feature = "typesense")]
            TenantSearchIndex::Typesense(index) => {
                index.search_index_file(scope, file_id, query).await
            }
            #[cfg(feature = "opensearch")]
            TenantSearchIndex::OpenSearch(index) => {
                index.search_index_file(scope, file_id, query).await
            }

            // Fallback error when no features are available
            #[cfg(not(any(feature = "typesense", feature = "opensearch")))]
            _ => panic!("no matching search index is available"),
        }
    }

    pub async fn add_data(&self, data: SearchIndexData) -> anyhow::Result<()> {
        match self {
            #[cfg(feature = "typesense")]
            TenantSearchIndex::Typesense(index) => index.add_data(data).await,
            #[cfg(feature = "opensearch")]
            TenantSearchIndex::OpenSearch(index) => index.add_data(data).await,

            // Fallback error when no features are available
            #[cfg(not(any(feature = "typesense", feature = "opensearch")))]
            _ => panic!("no matching search index is available"),
        }
    }

    pub async fn bulk_add_data(&self, data: Vec<SearchIndexData>) -> anyhow::Result<()> {
        match self {
            #[cfg(feature = "typesense")]
            TenantSearchIndex::Typesense(index) => index.bulk_add_data(data).await,
            #[cfg(feature = "opensearch")]
            TenantSearchIndex::OpenSearch(index) => index.bulk_add_data(data).await,

            // Fallback error when no features are available
            #[cfg(not(any(feature = "typesense", feature = "opensearch")))]
            _ => panic!("no matching search index is available"),
        }
    }

    pub async fn update_data(
        &self,
        item_id: Uuid,
        data: UpdateSearchIndexData,
    ) -> anyhow::Result<()> {
        match self {
            #[cfg(feature = "typesense")]
            TenantSearchIndex::Typesense(index) => index.update_data(item_id, data).await,
            #[cfg(feature = "opensearch")]
            TenantSearchIndex::OpenSearch(index) => index.update_data(item_id, data).await,

            // Fallback error when no features are available
            #[cfg(not(any(feature = "typesense", feature = "opensearch")))]
            _ => panic!("no matching search index is available"),
        }
    }

    pub async fn delete_data(&self, id: Uuid) -> anyhow::Result<()> {
        match self {
            #[cfg(feature = "typesense")]
            TenantSearchIndex::Typesense(index) => index.delete_data(id).await,
            #[cfg(feature = "opensearch")]
            TenantSearchIndex::OpenSearch(index) => index.delete_data(id).await,

            // Fallback error when no features are available
            #[cfg(not(any(feature = "typesense", feature = "opensearch")))]
            _ => panic!("no matching search index is available"),
        }
    }

    pub async fn delete_by_scope(&self, scope: DocumentBoxScopeRaw) -> anyhow::Result<()> {
        match self {
            #[cfg(feature = "typesense")]
            TenantSearchIndex::Typesense(index) => index.delete_by_scope(scope).await,
            #[cfg(feature = "opensearch")]
            TenantSearchIndex::OpenSearch(index) => index.delete_by_scope(scope).await,

            // Fallback error when no features are available
            #[cfg(not(any(feature = "typesense", feature = "opensearch")))]
            _ => panic!("no matching search index is available"),
        }
    }

    pub async fn apply_migration(&self, name: &str) -> anyhow::Result<()> {
        match self {
            #[cfg(feature = "typesense")]
            TenantSearchIndex::Typesense(index) => index.apply_migration(name).await,
            #[cfg(feature = "opensearch")]
            TenantSearchIndex::OpenSearch(index) => index.apply_migration(name).await,

            // Fallback error when no features are available
            #[cfg(not(any(feature = "typesense", feature = "opensearch")))]
            _ => panic!("no matching search index is available"),
        }
    }
}

pub(crate) trait SearchIndex: Send + Sync + 'static {
    /// Creates a search index for the tenant
    async fn create_index(&self) -> anyhow::Result<()>;

    /// Deletes the search index for the tenant
    async fn delete_index(&self) -> anyhow::Result<()>;

    /// Searches the index for the provided query
    async fn search_index(
        &self,
        scope: &[DocumentBoxScopeRaw],
        query: SearchRequest,
        folder_children: Option<Vec<FolderId>>,
    ) -> anyhow::Result<SearchResults>;

    /// Searches the index for matches scoped to a specific file
    async fn search_index_file(
        &self,
        scope: &DocumentBoxScopeRaw,
        file_id: FileId,
        query: FileSearchRequest,
    ) -> anyhow::Result<FileSearchResults>;

    /// Adds the provided data to the search index
    async fn add_data(&self, data: SearchIndexData) -> anyhow::Result<()>;

    /// Adds the provided data to the search index
    async fn bulk_add_data(&self, data: Vec<SearchIndexData>) -> anyhow::Result<()>;

    /// Updates the provided data in the search index
    async fn update_data(&self, item_id: Uuid, data: UpdateSearchIndexData) -> anyhow::Result<()>;

    /// Deletes the provided data from the search index
    async fn delete_data(&self, id: Uuid) -> anyhow::Result<()>;

    /// Deletes all data contained within the specified `scope`
    async fn delete_by_scope(&self, scope: DocumentBoxScopeRaw) -> anyhow::Result<()>;

    /// Apply a migration by name
    async fn apply_migration(&self, name: &str) -> anyhow::Result<()>;
}
