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
            DocboxSearchDateRange, DocboxSearchFilters, DocboxSearchMatchRanked,
            count_search_file_pages, delete_file_pages_by_file_id, delete_file_pages_by_scope,
            search_file_pages,
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatabaseSearchConfig {}

impl DatabaseSearchConfig {
    pub fn from_env() -> Result<Self, DatabaseSearchIndexFactoryError> {
        Ok(Self {})
    }
}

#[derive(Clone)]
pub struct DatabaseSearchIndexFactory {
    db: Arc<DatabasePoolCache>,
}

impl DatabaseSearchIndexFactory {
    pub fn from_config(
        db: Arc<DatabasePoolCache>,
        _config: DatabaseSearchConfig,
    ) -> Result<Self, DatabaseSearchIndexFactoryError> {
        Ok(Self { db })
    }

    pub fn create_search_index(&self, tenant: &Tenant) -> DatabaseSearchIndex {
        DatabaseSearchIndex {
            db: IndexDatabaseSource::Pools {
                db: self.db.clone(),
                tenant: Arc::new(tenant.clone()),
            },
        }
    }
}

#[derive(Clone)]
pub struct DatabaseSearchIndex {
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

const TENANT_MIGRATIONS: &[(&str, &str)] = &[
    (
        "m1_create_additional_indexes",
        include_str!("./migrations/m1_create_additional_indexes.sql"),
    ),
    (
        "m2_search_create_files_pages_table",
        include_str!("./migrations/m2_search_create_files_pages_table.sql"),
    ),
    (
        "m3_create_tsvector_columns",
        include_str!("./migrations/m3_create_tsvector_columns.sql"),
    ),
    (
        "m4_search_functions_and_types",
        include_str!("./migrations/m4_search_functions_and_types.sql"),
    ),
];

impl DatabaseSearchIndex {
    pub fn from_pool(db: DbPool) -> Self {
        Self {
            db: IndexDatabaseSource::Pool(db),
        }
    }

    pub async fn acquire_db(&self) -> Result<DbPool, SearchError> {
        match &self.db {
            IndexDatabaseSource::Pools { db, tenant } => {
                let db = db.get_tenant_pool(tenant).await.map_err(|error| {
                    tracing::error!(?error, "failed to acquire database for searching");
                    DatabaseSearchError::AcquireDatabase
                })?;
                Ok(db)
            }
            IndexDatabaseSource::Pool(db) => Ok(db.clone()),
        }
    }

    /// Close the associated tenant database pool
    pub async fn close(&self) {
        if let IndexDatabaseSource::Pools { db, tenant } = &self.db {
            db.close_tenant_pool(tenant).await;
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

    async fn search_index(
        &self,
        scopes: &[DocumentBoxScopeRaw],
        query: SearchRequest,
        folder_children: Option<Vec<FolderId>>,
    ) -> Result<crate::models::SearchResults, SearchError> {
        let db = self.acquire_db().await?;

        let query_text = query.query.unwrap_or_default();

        let results: Vec<DocboxSearchMatchRanked> = sqlx::query_as(
            r#"
    SELECT *
    FROM resolve_search_results(
        $1,
        plainto_tsquery('english', $1),
        $2,
        $3,
        $4,
        $5
    )
    LIMIT $6
    OFFSET $7"#,
        )
        .bind(query_text)
        .bind(DocboxSearchFilters {
            document_boxes: scopes.to_vec(),
            folder_children,
            include_name: query.include_name,
            include_content: query.include_content,
            created_at: query.created_at.map(|value| DocboxSearchDateRange {
                start: value.start,
                end: value.end,
            }),
            created_by: query.created_by,
        })
        .bind(query.mime.map(|value| value.0.to_string()))
        .bind(query.max_pages.unwrap_or(3) as i32)
        .bind(query.pages_offset.unwrap_or_default() as i32)
        .bind(query.size.unwrap_or(50) as i32)
        .bind(query.offset.unwrap_or(0) as i32)
        .fetch_all(&db)
        .await
        .map_err(|error| {
            tracing::error!(?error, "failed to search index");
            DatabaseSearchError::SearchIndex
        })?;

        let total_hits = results
            .first()
            .map(|result| result.total_count)
            .unwrap_or_default();

        let results = results
            .into_iter()
            .filter_map(|result| {
                let rank = result.rank;
                let result = result.search_match;

                let item_ty = match result.item_type.as_str() {
                    "File" => SearchIndexType::File,
                    "Folder" => SearchIndexType::Folder,
                    "Link" => SearchIndexType::Link,
                    // Unknown type error, should never occur but must be handled
                    _ => return None,
                };

                Some(FlattenedItemResult {
                    item_ty,
                    item_id: result.item_id,
                    document_box: result.document_box,
                    page_matches: result
                        .page_matches
                        .into_iter()
                        .map(|result| PageResult {
                            matches: vec![result.matched],
                            page: result.page as u64,
                        })
                        .collect(),
                    total_hits: result.total_hits as u64,
                    score: SearchScore::Float(rank as f32),
                    name_match: result.name_match,
                    content_match: result.content_match,
                })
            })
            .collect();

        Ok(SearchResults {
            total_hits: total_hits as u64,
            results,
        })
    }

    async fn search_index_file(
        &self,
        scope: &DocumentBoxScopeRaw,
        file_id: FileId,
        query: FileSearchRequest,
    ) -> Result<crate::models::FileSearchResults, SearchError> {
        let db = self.acquire_db().await?;
        let query_text = query.query.unwrap_or_default();
        let total_pages = count_search_file_pages(&db, scope, file_id, &query_text)
            .await
            .map_err(|error| {
                tracing::error!(?error, "failed to count search file pages");
                DatabaseSearchError::CountFilePages
            })?;
        let pages = search_file_pages(
            &db,
            scope,
            file_id,
            &query_text,
            query.limit.unwrap_or(50) as i64,
            query.offset.unwrap_or(0) as i64,
        )
        .await
        .map_err(|error| {
            tracing::error!(?error, "failed to search file pages");
            DatabaseSearchError::SearchFilePages
        })?;

        Ok(FileSearchResults {
            total_hits: total_pages.count as u64,
            results: pages
                .into_iter()
                .map(|page| PageResult {
                    page: page.page as u64,
                    matches: vec![page.highlighted_content],
                })
                .collect(),
        })
    }

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

            if let Err(error) = query.execute(&db).await {
                tracing::error!(?error, "failed to add search data");
                return Err(DatabaseSearchError::AddData.into());
            }
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

    async fn delete_data(&self, id: uuid::Uuid) -> Result<(), SearchError> {
        let db = self.acquire_db().await?;
        delete_file_pages_by_file_id(&db, id)
            .await
            .map_err(|error| {
                tracing::error!(?error, "failed to delete search data by id");
                DatabaseSearchError::DeleteData
            })?;
        Ok(())
    }

    async fn delete_by_scope(&self, scope: DocumentBoxScopeRawRef<'_>) -> Result<(), SearchError> {
        let db = self.acquire_db().await?;
        delete_file_pages_by_scope(&db, scope)
            .await
            .map_err(|error| {
                tracing::error!(?error, "failed to delete search data by scope");
                DatabaseSearchError::DeleteData
            })?;
        Ok(())
    }

    async fn get_pending_migrations(
        &self,
        applied_names: Vec<String>,
    ) -> Result<Vec<String>, SearchError> {
        let pending = TENANT_MIGRATIONS
            .iter()
            .filter(|(migration_name, _migration)| {
                // Skip already applied migrations
                !applied_names
                    .iter()
                    .any(|applied_migration| applied_migration.eq(migration_name))
            })
            .map(|(migration_name, _migration)| migration_name.to_string())
            .collect();

        Ok(pending)
    }

    async fn apply_migration(
        &self,
        _tenant: &docbox_database::models::tenant::Tenant,
        _root_t: &mut docbox_database::DbTransaction<'_>,
        t: &mut docbox_database::DbTransaction<'_>,
        name: &str,
    ) -> Result<(), SearchError> {
        let (_, migration) = TENANT_MIGRATIONS
            .iter()
            .find(|(migration_name, _)| name.eq(*migration_name))
            .ok_or(DatabaseSearchError::MigrationNotFound)?;

        // Apply the migration
        docbox_database::migrations::apply_migration(t, name, migration)
            .await
            .map_err(|error| {
                tracing::error!(?error, "failed to apply migration");
                DatabaseSearchError::ApplyMigration
            })?;

        Ok(())
    }
}
