use docbox_database::models::{document_box::DocumentBoxScope, folder::FolderId, tenant::Tenant};
use models::{SearchIndexData, SearchRequest, SearchResults, UpdateSearchIndexData};
use os::{OpenSearchIndex, OpenSearchIndexFactory, TenantSearchIndexName};
use uuid::Uuid;

pub mod models;
pub mod os;
pub mod os_models;

#[derive(Clone)]
pub struct SearchIndexFactory {
    os: OpenSearchIndexFactory,
}

impl SearchIndexFactory {
    pub fn new(os: OpenSearchIndexFactory) -> Self {
        Self { os }
    }

    /// Create a new "OpenSearch" search index for the tenant
    pub fn create_search_index(&self, tenant: &Tenant) -> TenantSearchIndex {
        let search_index = TenantSearchIndexName::from_tenant(tenant);
        TenantSearchIndex::OpenSearch(self.os.create_search_index(search_index))
    }
}

pub enum TenantSearchIndex {
    OpenSearch(OpenSearchIndex),
}

impl TenantSearchIndex {
    pub async fn create_index(&self) -> anyhow::Result<()> {
        match self {
            TenantSearchIndex::OpenSearch(index) => index.create_index().await,
        }
    }

    pub async fn delete_index(&self) -> anyhow::Result<()> {
        match self {
            TenantSearchIndex::OpenSearch(index) => index.delete_index().await,
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
        }
    }

    pub async fn add_data(&self, data: SearchIndexData) -> anyhow::Result<()> {
        match self {
            TenantSearchIndex::OpenSearch(index) => index.add_data(data).await,
        }
    }

    pub async fn update_data(
        &self,
        item_id: Uuid,
        data: UpdateSearchIndexData,
    ) -> anyhow::Result<()> {
        match self {
            TenantSearchIndex::OpenSearch(index) => index.update_data(item_id, data).await,
        }
    }

    pub async fn delete_data(&self, id: Uuid) -> anyhow::Result<()> {
        match self {
            TenantSearchIndex::OpenSearch(index) => index.delete_data(id).await,
        }
    }

    pub async fn delete_by_scope(&self, scope: DocumentBoxScope) -> anyhow::Result<()> {
        match self {
            TenantSearchIndex::OpenSearch(index) => index.delete_by_scope(scope).await,
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
