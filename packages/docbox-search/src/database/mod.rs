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
            DbSearchResult, count_search_file_pages, delete_file_pages_by_file_id,
            delete_file_pages_by_scope, search_file_pages,
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
            db: self.db.clone(),
            tenant: Arc::new(tenant.clone()),
        }
    }
}

#[derive(Clone)]
pub struct DatabaseSearchIndex {
    db: Arc<DatabasePoolCache>,
    tenant: Arc<Tenant>,
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
];

impl DatabaseSearchIndex {
    pub async fn acquire_db(&self) -> Result<DbPool, SearchError> {
        let db = self
            .db
            .get_tenant_pool(&self.tenant)
            .await
            .map_err(|error| {
                tracing::error!(?error, "failed to acquire database for searching");
                DatabaseSearchError::AcquireDatabase
            })?;
        Ok(db)
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

        let results: Vec<DbSearchResult> = sqlx::query_as(
            r#"
WITH
    "query_data" AS (
        SELECT plainto_tsquery('english', $1) AS "ts_query"
    ),

    -- Search links
    "link_matches" AS (
        SELECT
            'Link' AS "item_type",
            "link"."id" AS "item_id",
            "folder"."document_box" AS "document_box",
            ($3::BOOLEAN AND "link"."name_tsv" @@ "query_data"."ts_query") AS "name_match_tsv",
            ts_rank("link"."name_tsv", "query_data"."ts_query") AS "name_match_tsv_rank",
            ($3::BOOLEAN AND "link"."name" ILIKE '%' || $1 || '%') AS "name_match",
            ($4::BOOLEAN AND "link"."value" ILIKE '%' || $1 || '%') AS "content_match",
            0::FLOAT8 as "content_rank",
            0::INT AS "total_hits",
            '[]'::json AS "page_matches",
            "link"."created_at" AS "created_at"
        FROM "docbox_links" "link"
        CROSS JOIN "query_data"
        LEFT JOIN "docbox_folders" "folder" ON "link"."folder_id" = "folder"."id"
        WHERE "folder"."document_box" = ANY($2)
            AND ($6 IS NULL OR "link"."created_at" >= $6)
            AND ($7 IS NULL OR "link"."created_at" <= $7)
            AND ($8 IS NULL OR "link"."created_by" = $8)
            AND ($9 IS NULL OR "link"."folder_id" = ANY($9))
    ),

    -- Search folders
    "folder_matches" AS (
        SELECT
            'Folder' AS "item_type",
            "folder"."id" AS "item_id",
            "folder"."document_box" AS "document_box",
            ($3::BOOLEAN AND "folder"."name_tsv" @@ "query_data"."ts_query") AS "name_match_tsv",
            ts_rank("folder"."name_tsv", "query_data"."ts_query") AS "name_match_tsv_rank",
            ($3::BOOLEAN AND "folder"."name" ILIKE '%' || $1 || '%') AS "name_match",
            FALSE as "content_match",
            0::FLOAT8 as "content_rank",
            0::INT AS "total_hits",
            '[]'::json AS "page_matches",
            "folder"."created_at" AS "created_at"
        FROM "docbox_folders" "folder"
        CROSS JOIN "query_data"
        WHERE "folder"."document_box" = ANY($2)
            AND ($6 IS NULL OR "folder"."created_at" >= $6)
            AND ($7 IS NULL OR "folder"."created_at" <= $7)
            AND ($8 IS NULL OR "folder"."created_by" = $8)
            AND ($9 IS NULL OR "folder"."folder_id" = ANY($9))
    ),

    -- Search files
    "file_matches" AS (
        SELECT
            'File' AS "item_type",
            "file"."id" AS "item_id",
            "folder"."document_box" AS "document_box",
            ($3::BOOLEAN AND "file"."name_tsv" @@ "query_data"."ts_query") AS "name_match_tsv",
            ts_rank("file"."name_tsv", "query_data"."ts_query") AS "name_match_tsv_rank",
            ($3::BOOLEAN AND "file"."name" ILIKE '%' || $1 || '%') AS "name_match",
            ($4::BOOLEAN AND COUNT("pages"."page") > 0) AS "content_match",
            COALESCE(AVG("pages"."content_match_rank"), 0) as "content_rank",
            COALESCE(MAX("pages"."total_hits"), 0) AS "total_hits",
            (COALESCE(
                json_agg(
                    json_build_object(
                        'page', "pages"."page",
                        'matched', ts_headline('english', "pages"."content", "query_data"."ts_query", 'StartSel=<em>, StopSel=</em>')
                    )
                    ORDER BY "pages"."content_match_rank" DESC, "pages"."page" ASC
                )  FILTER (WHERE "pages"."page" IS NOT NULL),
                '[]'::json
            )) AS "page_matches",
            "file"."created_at" AS "created_at"
        FROM "docbox_files" "file"
        CROSS JOIN "query_data"
        LEFT JOIN "docbox_folders" "folder"
            ON "file"."folder_id" = "folder"."id" AND "folder"."document_box" = ANY($2)
        LEFT JOIN LATERAL (
            -- Match the page content
            WITH "page_data" AS (
                SELECT
                    "p".*,
                    "p"."content_tsv" @@ "query_data"."ts_query" AS "content_match_tsv",
                    "p"."content" ILIKE '%' || $1 || '%' AS "content_match",
                    COUNT(*) OVER () AS "total_hits",
                    (ts_rank("p"."content_tsv", "query_data"."ts_query")
                     + CASE WHEN "p"."content" ILIKE '%' || $1 || '%' THEN 1.0 ELSE 0 END -- Boost result for ILIKE content matches
                    ) AS "content_match_rank"
                FROM "docbox_files_pages" "p"
                WHERE "p"."file_id" = "file"."id"
            )
            SELECT *
            FROM "page_data"
            WHERE "content_match" OR "content_match_tsv"
            ORDER BY "content_match_rank" DESC, "page" ASC
            LIMIT $10::INT
            OFFSET $11::INT
        ) "pages" ON $4::BOOLEAN
        WHERE "folder"."document_box" = ANY($2)
            AND ($5 IS NULL OR "file"."mime" = $5)
            AND ($6 IS NULL OR "file"."created_at" >= $6)
            AND ($7 IS NULL OR "file"."created_at" <= $7)
            AND ($8 IS NULL OR "file"."created_by" = $8)
            AND ($9 IS NULL OR "file"."folder_id" = ANY($9))

        GROUP BY "file"."id", "folder"."document_box", "query_data"."ts_query"
    ),

    "results" AS (
        SELECT *
        FROM "link_matches"
        WHERE "name_match" OR "name_match_tsv" OR "content_match"

        UNION ALL

        SELECT *
        FROM "folder_matches"
        WHERE "name_match" OR "name_match_tsv" OR "content_match"

        UNION ALL

        SELECT *
        FROM "file_matches"
        WHERE "name_match" OR "name_match_tsv" OR "content_match"
    )

    (
        SELECT *,
        ("name_match_tsv_rank"
         + "content_rank"
         + CASE WHEN "name_match" THEN 1.0 ELSE 0 END -- Boost result for ILIKE name matches
         + CASE WHEN "item_type" = 'Link' AND "content_match" THEN 1.0 ELSE 0 END -- Boost link content matches
        ) AS "rank",
        COUNT("item_id") OVER() as "total_count"
        FROM "results"
        WHERE "name_match" OR "name_match_tsv" OR "content_match"
    )
    ORDER BY "rank" DESC, "created_at" DESC
    LIMIT $12
    OFFSET $13"#,
        )
        .bind(query_text)
        .bind(scopes)
        .bind(query.include_name)
        .bind(query.include_content)
        .bind(query.mime.map(|value| value.0.to_string()))
        .bind(query.created_at.as_ref().map(|created_at| created_at.start))
        .bind(query.created_at.as_ref().map(|created_at| created_at.end))
        .bind(query.created_by)
        .bind(folder_children)
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
                    score: SearchScore::Float(result.rank as f32),
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
