//! # Database
//!
//! Database backed search, uses the postgres backend directly as a search index.
//!
//! When using this search type additional tables and indexes are added in order to store the
//! page text contents for files in the database, it also adds additional columns to
//! other tables to provide tsvector variants to allow fast full text search.
//!
//! This is a good backend to choose if you don't wish to have a dedicated search service
//! running to manage a copy of your data, you can instead store it along side the metadata
//! inside your postgres database.

use crate::{
    SearchError, SearchIndex,
    models::{
        FileSearchRequest, FileSearchResults, FlattenedItemResult, PageResult, SearchIndexData,
        SearchIndexType, SearchRequest, SearchResults, SearchScore,
    },
};
use docbox_database::{
    DatabasePoolCache, DbPool,
    models::{
        document_box::{DocumentBoxScopeRaw, DocumentBoxScopeRawRef},
        file::FileId,
        folder::FolderId,
        search::{
            DocboxSearchDateRange, DocboxSearchFilters, DocboxSearchItemType,
            DocboxSearchMatchRanked, DocboxSearchPageMatch, SearchOptions,
            delete_file_pages_by_file_id, delete_file_pages_by_scope, search, search_file_pages,
        },
        tenant::Tenant,
    },
    sqlx,
};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::{sync::Arc, vec};

pub use error::{DatabaseSearchError, DatabaseSearchIndexFactoryError};

pub mod error;
mod migrations;

/// Configuration for a database backend search index
#[derive(Default, Debug, Clone, Deserialize, Serialize)]
pub struct DatabaseSearchConfig {}

impl DatabaseSearchConfig {
    /// Load the configuration from environment variables
    pub fn from_env() -> Result<Self, DatabaseSearchIndexFactoryError> {
        Ok(Self {})
    }
}

/// Factory for producing [DatabaseSearchIndex]'s for tenants
#[derive(Clone)]
pub struct DatabaseSearchIndexFactory {
    db: Arc<DatabasePoolCache>,
}

impl DatabaseSearchIndexFactory {
    // Create a [DatabaseSearchIndexFactory] from a `db` pool cache and
    // the provided configuration `config`
    pub fn from_config(
        db: Arc<DatabasePoolCache>,
        config: DatabaseSearchConfig,
    ) -> Result<Self, DatabaseSearchIndexFactoryError> {
        _ = config;

        Ok(Self { db })
    }

    /// Create a search index for the provided `tenant`
    pub fn create_search_index(&self, tenant: &Tenant) -> DatabaseSearchIndex {
        DatabaseSearchIndex {
            db: IndexDatabaseSource::Pools {
                db: self.db.clone(),
                tenant: Arc::new(tenant.clone()),
            },
        }
    }
}

/// Database backend search index
#[derive(Clone)]
pub struct DatabaseSearchIndex {
    /// Underlying database source
    db: IndexDatabaseSource,
}

#[derive(Clone)]
pub enum IndexDatabaseSource {
    /// Database source backed by the database pool cache
    Pools {
        /// The cache for producing databases
        db: Arc<DatabasePoolCache>,
        /// The tenant the search index is for
        tenant: Arc<Tenant>,
    },
    /// Singular database pool backed implementation for testing
    Pool(DbPool),
}

impl DatabaseSearchIndex {
    /// Create a search index from a db pool
    pub fn from_pool(db: DbPool) -> Self {
        Self {
            db: IndexDatabaseSource::Pool(db),
        }
    }

    /// Acquire a database connection
    async fn acquire_db(&self) -> Result<DbPool, SearchError> {
        match &self.db {
            IndexDatabaseSource::Pools { db, tenant } => {
                let db = db
                    .get_tenant_pool(tenant)
                    .await
                    .inspect_err(|error| {
                        tracing::error!(?error, "failed to acquire database for searching")
                    })
                    .map_err(DatabaseSearchError::AcquireDatabase)?;
                Ok(db)
            }
            IndexDatabaseSource::Pool(db) => Ok(db.clone()),
        }
    }

    /// Close the associated tenant database pool
    pub async fn close(&self) {
        match &self.db {
            IndexDatabaseSource::Pools { db, tenant } => {
                db.close_tenant_pool(tenant).await;
            }
            IndexDatabaseSource::Pool(pool) => {
                pool.close().await;
            }
        }
    }
}

impl SearchIndex for DatabaseSearchIndex {
    async fn create_index(&self) -> Result<(), SearchError> {
        // No-op, creation is handled in the migration running phase
        Ok(())
    }

    async fn index_exists(&self) -> Result<bool, SearchError> {
        // Since "index_exists" is used by the management interface to detect
        // if the index has been already created, in this case we want to always
        // report false so that it doesn't think the index exists
        // (Even though if the tenant exists then the index exists)
        Ok(false)
    }

    async fn delete_index(&self) -> Result<(), SearchError> {
        // No-op
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    async fn search_index(
        &self,
        scopes: &[DocumentBoxScopeRaw],
        query: SearchRequest,
        folder_children: Option<Vec<FolderId>>,
    ) -> Result<crate::models::SearchResults, SearchError> {
        let db = self.acquire_db().await?;

        let query_text = query.query.unwrap_or_default();

        let mime = query.mime.map(|value| value.0.to_string());

        let max_pages = query.max_pages.unwrap_or(3) as i64;
        let pages_offset = query.pages_offset.unwrap_or_default() as i64;

        let limit = query.size.unwrap_or(50) as i64;
        let offset = query.offset.unwrap_or_default() as i64;

        let filters = DocboxSearchFilters {
            document_boxes: scopes.to_vec(),
            folder_children,
            include_name: query.include_name,
            include_content: query.include_content,
            created_at: query.created_at.map(|value| DocboxSearchDateRange {
                start: value.start,
                end: value.end,
            }),
            created_by: query.created_by,
            mime,
        };

        let results = search(
            &db,
            SearchOptions {
                query: query_text,
                filters,
                max_pages,
                pages_offset,
                limit,
                offset,
            },
        )
        .await
        .inspect_err(|error| tracing::error!(?error, "failed to search index"))
        .map_err(DatabaseSearchError::SearchIndex)?;

        let total_hits = results
            .first()
            .map(|result| result.total_count)
            .unwrap_or_default() as u64;

        let results = results.into_iter().map(FlattenedItemResult::from).collect();

        Ok(SearchResults {
            total_hits,
            results,
        })
    }

    #[tracing::instrument(skip(self))]
    async fn search_index_file(
        &self,
        scope: &DocumentBoxScopeRaw,
        file_id: FileId,
        query: FileSearchRequest,
    ) -> Result<crate::models::FileSearchResults, SearchError> {
        let db = self.acquire_db().await?;
        let query_text = query.query.unwrap_or_default();

        let limit = query.limit.unwrap_or(50) as i64;
        let offset = query.offset.unwrap_or_default() as i64;

        let pages = search_file_pages(&db, scope, file_id, &query_text, limit, offset)
            .await
            .map_err(|error| {
                tracing::error!(?error, "failed to search file pages");
                DatabaseSearchError::SearchFilePages
            })?;

        let total_hits = pages
            .first()
            .map(|value| value.total_hits)
            .unwrap_or_default() as u64;

        Ok(FileSearchResults {
            total_hits,
            results: pages.into_iter().map(PageResult::from).collect(),
        })
    }

    #[tracing::instrument(skip_all)]
    async fn add_data(&self, data: Vec<SearchIndexData>) -> Result<(), SearchError> {
        let db = self.acquire_db().await?;

        for item in data {
            let pages = match item.pages {
                Some(value) => value,
                // Skip anything without pages
                None => continue,
            };

            if pages.is_empty() {
                continue;
            }

            let values = pages
                .iter()
                .enumerate()
                .map(|(index, _page)| format!("($1, ${}, ${})", 2 + index * 2, 3 + index * 2))
                .join(",");

            let query = format!(
                r#"INSERT INTO "docbox_files_pages" ("file_id", "page", "content") VALUES {values}"#
            );

            let mut query = sqlx::query(&query)
                // Shared amongst all values
                .bind(item.item_id);

            for page in pages {
                query = query.bind(page.page as i32).bind(page.content);
            }

            query
                .execute(&db)
                .await
                .inspect_err(|error| tracing::error!(?error, "failed to add search data"))
                .map_err(DatabaseSearchError::AddData)?;
        }

        Ok(())
    }

    async fn update_data(
        &self,
        _item_id: uuid::Uuid,
        _data: crate::models::UpdateSearchIndexData,
    ) -> Result<(), SearchError> {
        // No-op: Currently page data is never updated, and since this search implementation sources all other
        // data directly from the database it already has a copy of everything it needs so no changes need to be made
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    async fn delete_data(&self, id: uuid::Uuid) -> Result<(), SearchError> {
        let db = self.acquire_db().await?;
        delete_file_pages_by_file_id(&db, id)
            .await
            .inspect_err(|error| tracing::error!(?error, "failed to delete search data by id"))
            .map_err(DatabaseSearchError::DeleteData)?;
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    async fn delete_by_scope(&self, scope: DocumentBoxScopeRawRef<'_>) -> Result<(), SearchError> {
        let db = self.acquire_db().await?;
        delete_file_pages_by_scope(&db, scope)
            .await
            .inspect_err(|error| tracing::error!(?error, "failed to delete search data by scope"))
            .map_err(DatabaseSearchError::DeleteData)?;
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    async fn get_pending_migrations(
        &self,
        applied_names: Vec<String>,
    ) -> Result<Vec<String>, SearchError> {
        let pending = migrations::get_pending_migrations(applied_names);
        Ok(pending)
    }

    #[tracing::instrument(skip(self))]
    async fn apply_migration(
        &self,
        _tenant: &docbox_database::models::tenant::Tenant,
        _root_t: &mut docbox_database::DbTransaction<'_>,
        t: &mut docbox_database::DbTransaction<'_>,
        name: &str,
    ) -> Result<(), SearchError> {
        migrations::apply_migration(t, name).await
    }
}

impl From<DocboxSearchMatchRanked> for FlattenedItemResult {
    fn from(value: DocboxSearchMatchRanked) -> Self {
        let DocboxSearchMatchRanked {
            search_match, rank, ..
        } = value;
        FlattenedItemResult {
            item_ty: search_match.item_type.into(),
            item_id: search_match.item_id,
            document_box: search_match.document_box,
            page_matches: search_match
                .page_matches
                .into_iter()
                .map(PageResult::from)
                .collect(),
            total_hits: search_match.total_hits as u64,
            score: SearchScore::Float(rank as f32),
            name_match: search_match.name_match,
            content_match: search_match.content_match,
        }
    }
}

impl From<DocboxSearchPageMatch> for PageResult {
    fn from(value: DocboxSearchPageMatch) -> Self {
        PageResult {
            matches: vec![value.matched],
            page: value.page as u64,
        }
    }
}

impl From<DocboxSearchItemType> for SearchIndexType {
    fn from(value: DocboxSearchItemType) -> Self {
        match value {
            DocboxSearchItemType::File => SearchIndexType::File,
            DocboxSearchItemType::Folder => SearchIndexType::Folder,
            DocboxSearchItemType::Link => SearchIndexType::Link,
        }
    }
}
