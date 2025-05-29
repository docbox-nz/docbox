use anyhow::Context;
use aws_config::SdkConfig;
use docbox_database::models::{document_box::DocumentBoxScope, folder::FolderId, tenant::Tenant};
use models::{SearchIndexData, SearchRequest, SearchResults, UpdateSearchIndexData};
use os::{create_open_search, OpenSearchIndex, OpenSearchIndexFactory, TenantSearchIndexName};
use reqwest::Url;
use typesense::{TypesenseIndex, TypesenseIndexFactory};
use uuid::Uuid;

pub mod models;
pub mod os;
pub mod os_models;
pub mod typesense;

#[derive(Clone)]
pub enum SearchIndexFactory {
    OpenSearch(OpenSearchIndexFactory),
    Typesense(TypesenseIndexFactory),
}

impl SearchIndexFactory {
    pub fn from_env(aws_config: &SdkConfig) -> anyhow::Result<Self> {
        match std::env::var("SEARCH_INDEX_FACTORY")
            .unwrap_or_else(|_| "opensearch".to_string())
            .as_str()
        {
            "typesense" => {
                tracing::debug!("using typesense search index");
                let typesense_url =
                    std::env::var("TYPESENSE_URL").context("missing TYPESENSE_URL env")?;
                let api_key =
                    std::env::var("TYPESENSE_API_KEY").context("missing TYPESENSE_API_KEY env")?;

                let typesense_factory = TypesenseIndexFactory::new(typesense_url, api_key)?;
                Ok(SearchIndexFactory::Typesense(typesense_factory))
            }
            _ => {
                tracing::debug!("using opensearch search index");
                // Setup opensearch
                let open_search_url = std::env::var("OPENSEARCH_URL")
                    // Map the error to an anyhow type
                    .context("missing OPENSEARCH_URL env")
                    // Parse the URL
                    .and_then(|url| Url::parse(&url).context("failed to parse OPENSEARCH_URL"))?;

                let open_search = create_open_search(aws_config, open_search_url)
                    .context("failed to create open search")?;

                let os_index_factory = OpenSearchIndexFactory::new(open_search);
                Ok(SearchIndexFactory::OpenSearch(os_index_factory))
            }
        }
    }

    /// Create a new "OpenSearch" search index for the tenant
    pub fn create_search_index(&self, tenant: &Tenant) -> TenantSearchIndex {
        match self {
            SearchIndexFactory::OpenSearch(factory) => {
                let search_index = TenantSearchIndexName::from_tenant(tenant);
                TenantSearchIndex::OpenSearch(factory.create_search_index(search_index))
            }
            SearchIndexFactory::Typesense(factory) => {
                let search_index = tenant.os_index_name.clone();
                TenantSearchIndex::Typesense(factory.create_search_index(search_index))
            }
        }
    }
}

pub enum TenantSearchIndex {
    OpenSearch(OpenSearchIndex),
    Typesense(TypesenseIndex),
}

impl TenantSearchIndex {
    pub async fn create_index(&self) -> anyhow::Result<()> {
        match self {
            TenantSearchIndex::OpenSearch(index) => index.create_index().await,
            TenantSearchIndex::Typesense(index) => index.create_index().await,
        }
    }

    pub async fn delete_index(&self) -> anyhow::Result<()> {
        match self {
            TenantSearchIndex::OpenSearch(index) => index.delete_index().await,
            TenantSearchIndex::Typesense(index) => index.delete_index().await,
        }
    }

    pub async fn search_index(
        &self,
        scope: &[DocumentBoxScope],
        query: SearchRequest,
        folder_children: Option<Vec<FolderId>>,
    ) -> anyhow::Result<SearchResults> {
        match self {
            TenantSearchIndex::OpenSearch(index) => {
                index.search_index(scope, query, folder_children).await
            }
            TenantSearchIndex::Typesense(index) => {
                index.search_index(scope, query, folder_children).await
            }
        }
    }

    pub async fn add_data(&self, data: SearchIndexData) -> anyhow::Result<()> {
        match self {
            TenantSearchIndex::OpenSearch(index) => index.add_data(data).await,
            TenantSearchIndex::Typesense(index) => index.add_data(data).await,
        }
    }

    pub async fn update_data(
        &self,
        item_id: Uuid,
        data: UpdateSearchIndexData,
    ) -> anyhow::Result<()> {
        match self {
            TenantSearchIndex::OpenSearch(index) => index.update_data(item_id, data).await,
            TenantSearchIndex::Typesense(index) => index.update_data(item_id, data).await,
        }
    }

    pub async fn delete_data(&self, id: Uuid) -> anyhow::Result<()> {
        match self {
            TenantSearchIndex::OpenSearch(index) => index.delete_data(id).await,
            TenantSearchIndex::Typesense(index) => index.delete_data(id).await,
        }
    }

    pub async fn delete_by_scope(&self, scope: DocumentBoxScope) -> anyhow::Result<()> {
        match self {
            TenantSearchIndex::OpenSearch(index) => index.delete_by_scope(scope).await,
            TenantSearchIndex::Typesense(index) => index.delete_by_scope(scope).await,
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
        scope: &[DocumentBoxScope],
        query: SearchRequest,
        folder_children: Option<Vec<FolderId>>,
    ) -> anyhow::Result<SearchResults>;

    /// Adds the provided data to the search index
    async fn add_data(&self, data: SearchIndexData) -> anyhow::Result<()>;

    /// Updates the provided data in the search index
    async fn update_data(&self, item_id: Uuid, data: UpdateSearchIndexData) -> anyhow::Result<()>;

    /// Deletes the provided data from the search index
    async fn delete_data(&self, id: Uuid) -> anyhow::Result<()>;

    async fn delete_by_scope(&self, scope: DocumentBoxScope) -> anyhow::Result<()>;
}
