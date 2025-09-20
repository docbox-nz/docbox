use aws_config::SdkConfig;
use chrono::Utc;
use docbox_database::{
    DatabasePoolCache, DbTransaction,
    models::{
        document_box::DocumentBoxScopeRaw,
        file::FileId,
        folder::FolderId,
        tenant::Tenant,
        tenant_migration::{CreateTenantMigration, TenantMigration},
    },
};
use docbox_secrets::SecretManager;
use models::{
    FileSearchRequest, FileSearchResults, SearchIndexData, SearchRequest, SearchResults,
    UpdateSearchIndexData,
};
use serde::{Deserialize, Serialize};
use std::{ops::DerefMut, sync::Arc};
use thiserror::Error;
use uuid::Uuid;

pub mod models;

pub use database::{DatabaseSearchConfig, DatabaseSearchError, DatabaseSearchIndexFactoryError};
pub use opensearch::{OpenSearchConfig, OpenSearchIndexFactoryError, OpenSearchSearchError};
pub use typesense::{
    TypesenseApiKey, TypesenseApiKeyProvider, TypesenseApiKeySecret, TypesenseIndexFactoryError,
    TypesenseSearchConfig, TypesenseSearchError,
};

mod database;
mod opensearch;
mod typesense;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum SearchIndexFactoryConfig {
    Typesense(typesense::TypesenseSearchConfig),
    OpenSearch(opensearch::OpenSearchConfig),
    Database(database::DatabaseSearchConfig),
}

#[derive(Debug, Error)]
pub enum SearchIndexFactoryError {
    #[error(transparent)]
    Typesense(#[from] typesense::TypesenseIndexFactoryError),
    #[error(transparent)]
    OpenSearch(#[from] opensearch::OpenSearchIndexFactoryError),
    #[error(transparent)]
    Database(#[from] database::DatabaseSearchIndexFactoryError),
    #[error("unknown search index factory type requested")]
    UnknownIndexFactory,
}

impl SearchIndexFactoryConfig {
    pub fn from_env() -> Result<Self, SearchIndexFactoryError> {
        let variant = std::env::var("DOCBOX_SEARCH_INDEX_FACTORY")
            .unwrap_or_else(|_| "database".to_string())
            .to_lowercase();
        match variant.as_str() {
            "open_search" | "opensearch" => opensearch::OpenSearchConfig::from_env()
                .map(Self::OpenSearch)
                .map_err(SearchIndexFactoryError::OpenSearch),

            "typesense" => typesense::TypesenseSearchConfig::from_env()
                .map(Self::Typesense)
                .map_err(SearchIndexFactoryError::Typesense),

            "database" => database::DatabaseSearchConfig::from_env()
                .map(Self::Database)
                .map_err(SearchIndexFactoryError::Database),

            // Unknown type requested
            _ => Err(SearchIndexFactoryError::UnknownIndexFactory),
        }
    }
}

#[derive(Clone)]
pub enum SearchIndexFactory {
    Typesense(typesense::TypesenseIndexFactory),
    OpenSearch(opensearch::OpenSearchIndexFactory),
    Database(database::DatabaseSearchIndexFactory),
}

impl SearchIndexFactory {
    /// Create a search index factory from the provided `config`
    pub fn from_config(
        aws_config: &SdkConfig,
        secrets: Arc<SecretManager>,
        db: Arc<DatabasePoolCache>,
        config: SearchIndexFactoryConfig,
    ) -> Result<Self, SearchIndexFactoryError> {
        match config {
            SearchIndexFactoryConfig::Typesense(config) => {
                tracing::debug!("using typesense search index");
                typesense::TypesenseIndexFactory::from_config(secrets, config)
                    .map(SearchIndexFactory::Typesense)
                    .map_err(SearchIndexFactoryError::Typesense)
            }

            SearchIndexFactoryConfig::OpenSearch(config) => {
                tracing::debug!("using opensearch search index");
                opensearch::OpenSearchIndexFactory::from_config(aws_config, config)
                    .map(SearchIndexFactory::OpenSearch)
                    .map_err(SearchIndexFactoryError::OpenSearch)
            }

            SearchIndexFactoryConfig::Database(config) => {
                tracing::debug!("using opensearch search index");
                database::DatabaseSearchIndexFactory::from_config(db, config)
                    .map(SearchIndexFactory::Database)
                    .map_err(SearchIndexFactoryError::Database)
            }
        }
    }

    /// Create a new "OpenSearch" search index for the tenant
    pub fn create_search_index(&self, tenant: &Tenant) -> TenantSearchIndex {
        match self {
            SearchIndexFactory::Typesense(factory) => {
                let search_index = tenant.os_index_name.clone();
                TenantSearchIndex::Typesense(factory.create_search_index(search_index))
            }

            SearchIndexFactory::OpenSearch(factory) => {
                let search_index = opensearch::TenantSearchIndexName::from_tenant(tenant);
                TenantSearchIndex::OpenSearch(factory.create_search_index(search_index))
            }

            SearchIndexFactory::Database(factory) => {
                TenantSearchIndex::Database(factory.create_search_index(tenant))
            }
        }
    }
}

#[derive(Clone)]
pub enum TenantSearchIndex {
    Typesense(typesense::TypesenseIndex),
    OpenSearch(opensearch::OpenSearchIndex),
    Database(database::DatabaseSearchIndex),
}

#[derive(Debug, Error)]
pub enum SearchError {
    #[error(transparent)]
    Typesense(#[from] typesense::TypesenseSearchError),
    #[error(transparent)]
    OpenSearch(#[from] opensearch::OpenSearchSearchError),
    #[error(transparent)]
    Database(#[from] database::DatabaseSearchError),
    #[error("failed to perform migration")]
    Migration,
}

impl TenantSearchIndex {
    /// Creates a search index for the tenant
    #[tracing::instrument(skip(self))]
    pub async fn create_index(&self) -> Result<(), SearchError> {
        match self {
            TenantSearchIndex::Typesense(index) => index.create_index().await,
            TenantSearchIndex::OpenSearch(index) => index.create_index().await,
            TenantSearchIndex::Database(index) => index.create_index().await,
        }
    }

    /// Deletes the search index for the tenant
    #[tracing::instrument(skip(self))]
    pub async fn delete_index(&self) -> Result<(), SearchError> {
        match self {
            TenantSearchIndex::Typesense(index) => index.delete_index().await,
            TenantSearchIndex::OpenSearch(index) => index.delete_index().await,
            TenantSearchIndex::Database(index) => index.delete_index().await,
        }
    }

    /// Searches the search index with the provided query
    #[tracing::instrument(skip(self))]
    pub async fn search_index(
        &self,
        scope: &[DocumentBoxScopeRaw],
        query: SearchRequest,
        folder_children: Option<Vec<FolderId>>,
    ) -> Result<SearchResults, SearchError> {
        match self {
            TenantSearchIndex::Typesense(index) => {
                index.search_index(scope, query, folder_children).await
            }
            TenantSearchIndex::OpenSearch(index) => {
                index.search_index(scope, query, folder_children).await
            }
            TenantSearchIndex::Database(index) => {
                index.search_index(scope, query, folder_children).await
            }
        }
    }

    /// Searches the index for matches scoped to a specific file
    #[tracing::instrument(skip(self))]
    pub async fn search_index_file(
        &self,
        scope: &DocumentBoxScopeRaw,
        file_id: FileId,
        query: FileSearchRequest,
    ) -> Result<FileSearchResults, SearchError> {
        match self {
            TenantSearchIndex::Typesense(index) => {
                index.search_index_file(scope, file_id, query).await
            }
            TenantSearchIndex::OpenSearch(index) => {
                index.search_index_file(scope, file_id, query).await
            }
            TenantSearchIndex::Database(index) => {
                index.search_index_file(scope, file_id, query).await
            }
        }
    }

    /// Adds the provided data to the search index
    #[tracing::instrument(skip(self))]
    pub async fn add_data(&self, data: Vec<SearchIndexData>) -> Result<(), SearchError> {
        match self {
            TenantSearchIndex::Typesense(index) => index.add_data(data).await,
            TenantSearchIndex::OpenSearch(index) => index.add_data(data).await,
            TenantSearchIndex::Database(index) => index.add_data(data).await,
        }
    }

    /// Updates the provided data in the search index
    #[tracing::instrument(skip(self))]
    pub async fn update_data(
        &self,
        item_id: Uuid,
        data: UpdateSearchIndexData,
    ) -> Result<(), SearchError> {
        match self {
            TenantSearchIndex::Typesense(index) => index.update_data(item_id, data).await,
            TenantSearchIndex::OpenSearch(index) => index.update_data(item_id, data).await,
            TenantSearchIndex::Database(index) => index.update_data(item_id, data).await,
        }
    }

    /// Deletes the provided data from the search index by `id`
    #[tracing::instrument(skip(self))]
    pub async fn delete_data(&self, id: Uuid) -> Result<(), SearchError> {
        match self {
            TenantSearchIndex::Typesense(index) => index.delete_data(id).await,
            TenantSearchIndex::OpenSearch(index) => index.delete_data(id).await,
            TenantSearchIndex::Database(index) => index.delete_data(id).await,
        }
    }

    /// Deletes all data contained within the specified `scope`
    #[tracing::instrument(skip(self))]
    pub async fn delete_by_scope(&self, scope: DocumentBoxScopeRaw) -> Result<(), SearchError> {
        match self {
            TenantSearchIndex::Typesense(index) => index.delete_by_scope(scope).await,
            TenantSearchIndex::OpenSearch(index) => index.delete_by_scope(scope).await,
            TenantSearchIndex::Database(index) => index.delete_by_scope(scope).await,
        }
    }

    /// Get all pending migrations based on the `applied_names` list of applied migrations
    #[tracing::instrument(skip(self))]
    pub async fn get_pending_migrations(
        &self,
        applied_names: Vec<String>,
    ) -> Result<Vec<String>, SearchError> {
        match self {
            TenantSearchIndex::Typesense(index) => {
                index.get_pending_migrations(applied_names).await
            }
            TenantSearchIndex::OpenSearch(index) => {
                index.get_pending_migrations(applied_names).await
            }
            TenantSearchIndex::Database(index) => index.get_pending_migrations(applied_names).await,
        }
    }

    /// Apply a specific migration for a `tenant` by `name`
    #[tracing::instrument(skip(self))]
    pub async fn apply_migration(
        &self,
        tenant: &Tenant,
        root_t: &mut DbTransaction<'_>,
        tenant_t: &mut DbTransaction<'_>,
        name: &str,
    ) -> Result<(), SearchError> {
        // Apply migration logic
        match self {
            TenantSearchIndex::Typesense(index) => {
                index
                    .apply_migration(tenant, root_t, tenant_t, name)
                    .await?
            }

            TenantSearchIndex::OpenSearch(index) => {
                index
                    .apply_migration(tenant, root_t, tenant_t, name)
                    .await?
            }

            TenantSearchIndex::Database(index) => {
                index
                    .apply_migration(tenant, root_t, tenant_t, name)
                    .await?
            }
        }

        // Store the applied migration
        TenantMigration::create(
            root_t.deref_mut(),
            CreateTenantMigration {
                tenant_id: tenant.id,
                env: tenant.env.clone(),
                name: name.to_string(),
                applied_at: Utc::now(),
            },
        )
        .await
        .map_err(|error| {
            tracing::error!(?error, "failed to create tenant migration");
            SearchError::Migration
        })?;

        Ok(())
    }

    /// Apply all pending migrations for a `tenant`
    ///
    /// When `target_migration_name` is specified only that target migration will
    /// be run
    #[tracing::instrument(skip_all, fields(?tenant, ?target_migration_name))]
    pub async fn apply_migrations(
        &self,
        tenant: &Tenant,
        root_t: &mut DbTransaction<'_>,
        tenant_t: &mut DbTransaction<'_>,
        target_migration_name: Option<&str>,
    ) -> Result<(), SearchError> {
        let applied_migrations =
            TenantMigration::find_by_tenant(root_t.deref_mut(), tenant.id, &tenant.env)
                .await
                .map_err(|error| {
                    tracing::error!(?error, "failed to query tenant migrations");
                    SearchError::Migration
                })?;
        let pending_migrations = self
            .get_pending_migrations(
                applied_migrations
                    .into_iter()
                    .map(|value| value.name)
                    .collect(),
            )
            .await?;

        for migration_name in pending_migrations {
            // If targeting a specific migration only apply the target one
            if target_migration_name
                .is_some_and(|target_migration_name| target_migration_name.ne(&migration_name))
            {
                continue;
            }

            // Apply the migration
            if let Err(error) = self
                .apply_migration(tenant, root_t, tenant_t, &migration_name)
                .await
            {
                tracing::error!(%migration_name, ?error, "failed to apply migration");
                return Err(error);
            }
        }

        Ok(())
    }
}

pub(crate) trait SearchIndex: Send + Sync + 'static {
    async fn create_index(&self) -> Result<(), SearchError>;

    async fn delete_index(&self) -> Result<(), SearchError>;

    async fn search_index(
        &self,
        scope: &[DocumentBoxScopeRaw],
        query: SearchRequest,
        folder_children: Option<Vec<FolderId>>,
    ) -> Result<SearchResults, SearchError>;

    async fn search_index_file(
        &self,
        scope: &DocumentBoxScopeRaw,
        file_id: FileId,
        query: FileSearchRequest,
    ) -> Result<FileSearchResults, SearchError>;

    async fn add_data(&self, data: Vec<SearchIndexData>) -> Result<(), SearchError>;

    async fn update_data(
        &self,
        item_id: Uuid,
        data: UpdateSearchIndexData,
    ) -> Result<(), SearchError>;

    async fn delete_data(&self, id: Uuid) -> Result<(), SearchError>;

    async fn delete_by_scope(&self, scope: DocumentBoxScopeRaw) -> Result<(), SearchError>;

    async fn get_pending_migrations(
        &self,
        applied_names: Vec<String>,
    ) -> Result<Vec<String>, SearchError>;

    async fn apply_migration(
        &self,
        tenant: &Tenant,
        root_t: &mut DbTransaction<'_>,
        t: &mut DbTransaction<'_>,
        name: &str,
    ) -> Result<(), SearchError>;
}
