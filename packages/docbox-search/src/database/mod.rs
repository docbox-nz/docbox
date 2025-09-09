//! # Database
//!
//! Database backed search

use std::sync::Arc;

use anyhow::{Context, Ok};
use docbox_database::{
    DatabasePoolCache,
    migrations::apply_tenant_migration,
    models::{
        document_box::DocumentBoxScopeRaw,
        file::FileId,
        folder::FolderId,
        search::{count_search_file_pages, search_file_pages},
        tenant::Tenant,
    },
    sqlx::{self},
};
use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::{
    SearchIndex,
    models::{
        FileSearchRequest, FileSearchResults, PageResult, SearchIndexData, SearchRequest,
        SearchResults,
    },
};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatabaseSearchConfig {}

impl DatabaseSearchConfig {
    pub fn from_env() -> anyhow::Result<Self> {
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
    ) -> anyhow::Result<Self> {
        Ok(Self { db })
    }

    pub fn new(db: Arc<DatabasePoolCache>) -> Self {
        Self { db }
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
];

impl SearchIndex for DatabaseSearchIndex {
    async fn create_index(&self) -> anyhow::Result<()> {
        // No-op, creation is handled in the migration running phase
        Ok(())
    }

    async fn delete_index(&self) -> anyhow::Result<()> {
        // No-op
        Ok(())
    }

    async fn search_index(
        &self,
        _scope: &[DocumentBoxScopeRaw],
        _query: SearchRequest,
        _folder_children: Option<Vec<FolderId>>,
    ) -> anyhow::Result<crate::models::SearchResults> {
        let total_hits = 0;
        let results = Vec::new();

        // TODO:

        Ok(SearchResults {
            total_hits,
            results,
        })
    }

    async fn search_index_file(
        &self,
        scope: &DocumentBoxScopeRaw,
        file_id: FileId,
        query: FileSearchRequest,
    ) -> anyhow::Result<crate::models::FileSearchResults> {
        let db = self.db.get_tenant_pool(&self.tenant).await?;
        let query_text = query.query.unwrap_or_default();
        let total_pages = count_search_file_pages(&db, scope, file_id, &query_text).await?;
        let pages = search_file_pages(
            &db,
            scope,
            file_id,
            &query_text,
            query.limit.unwrap_or(50) as i64,
            query.offset.unwrap_or(0) as i64,
        )
        .await?;

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

    async fn add_data(&self, data: SearchIndexData) -> anyhow::Result<()> {
        self.bulk_add_data(vec![data]).await
    }

    async fn bulk_add_data(&self, data: Vec<SearchIndexData>) -> anyhow::Result<()> {
        let db = self.db.get_tenant_pool(&self.tenant).await?;

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

            tracing::debug!(?pages);

            for page in pages {
                query = query.bind(page.page as i32).bind(page.content);
            }

            _ = query.execute(&db).await?;
        }

        Ok(())
    }

    async fn update_data(
        &self,
        _item_id: uuid::Uuid,
        _data: crate::models::UpdateSearchIndexData,
    ) -> anyhow::Result<()> {
        // Currently no-op, we only care about page data in this backend and currently no system ever updates that here
        Ok(())
    }

    async fn delete_data(&self, _id: uuid::Uuid) -> anyhow::Result<()> {
        // TODO:
        Ok(())
    }

    async fn delete_by_scope(&self, _scope: DocumentBoxScopeRaw) -> anyhow::Result<()> {
        // TODO:
        Ok(())
    }

    async fn get_pending_migrations(
        &self,
        applied_names: Vec<String>,
    ) -> anyhow::Result<Vec<String>> {
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
    ) -> anyhow::Result<()> {
        let (_, migration) = TENANT_MIGRATIONS
            .iter()
            .find(|(migration_name, _)| name.eq(*migration_name))
            .context("migration not found")?;

        // Apply the migration
        apply_tenant_migration(t, name, migration).await?;

        Ok(())
    }
}
