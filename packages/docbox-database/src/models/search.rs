use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use uuid::Uuid;

use crate::{
    DbPool, DbResult,
    models::document_box::{DocumentBoxScopeRaw, DocumentBoxScopeRawRef},
};

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct DbPageResult {
    pub page: i32,
    pub highlighted_content: String,
    pub rank: f32,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct DbPageCountResult {
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, sqlx::Type)]
#[sqlx(type_name = "docbox_search_page_match")]
pub struct DocboxSearchPageMatch {
    pub page: i64,
    pub matched: String,
    pub content_match_rank: f64,
    pub total_hits: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, sqlx::Type)]
#[sqlx(type_name = "docbox_search_date_range")]
pub struct DocboxSearchDateRange {
    pub start: Option<DateTime<Utc>>,
    pub end: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, sqlx::Type)]
#[sqlx(type_name = "docbox_search_filters")]
pub struct DocboxSearchFilters {
    pub document_boxes: Vec<String>,
    pub folder_children: Option<Vec<Uuid>>,
    pub include_name: bool,
    pub include_content: bool,
    pub created_at: Option<DocboxSearchDateRange>,
    pub created_by: Option<String>,
    pub mime: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "docbox_search_item_type")]
pub enum DocboxSearchItemType {
    File,
    Link,
    Folder,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, sqlx::Type)]
#[sqlx(type_name = "docbox_search_match")]
pub struct DocboxSearchMatch {
    pub item_type: DocboxSearchItemType,
    pub item_id: Uuid,
    pub document_box: String,
    pub name_match_tsv: bool,
    pub name_match_tsv_rank: f64,
    pub name_match: bool,
    pub content_match: bool,
    pub content_rank: f64,
    pub total_hits: i64,
    pub page_matches: Vec<DocboxSearchPageMatch>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, sqlx::Type)]
#[sqlx(type_name = "docbox_search_match_ranked")]
pub struct DocboxSearchMatchRanked {
    pub search_match: DocboxSearchMatch,
    pub rank: f64,
    pub total_count: i64,
}

pub struct SearchOptions {
    pub query: String,
    pub filters: DocboxSearchFilters,
    pub max_pages: i64,
    pub pages_offset: i64,
    pub limit: i64,
    pub offset: i64,
}

pub async fn search(db: &DbPool, options: SearchOptions) -> DbResult<Vec<DocboxSearchMatchRanked>> {
    sqlx::query_as(
        r#"
        SELECT * FROM docbox_search($1, plainto_tsquery('english', $1), $2, $3, $4)
        LIMIT $5
        OFFSET $6
    "#,
    )
    .bind(options.query)
    .bind(options.filters)
    .bind(options.max_pages)
    .bind(options.pages_offset)
    .bind(options.limit)
    .bind(options.offset)
    .fetch_all(db)
    .await
}

pub async fn search_file_pages(
    db: &DbPool,
    scope: &DocumentBoxScopeRaw,
    file_id: Uuid,
    query: &str,
    limit: i64,
    offset: i64,
) -> DbResult<Vec<DocboxSearchPageMatch>> {
    sqlx::query_as(r#"
        SELECT * FROM docbox_search_file_pages_with_scope($1, $2, $3, plainto_tsquery('english', $3))
        LIMIT $4
        OFFSET $5
    "#)
    .bind(scope)
    .bind(file_id)
    .bind(query)
    .bind(limit)
    .bind(offset)
    .fetch_all(db)
    .await
}

pub async fn delete_file_pages_by_scope(
    db: &DbPool,
    scope: DocumentBoxScopeRawRef<'_>,
) -> DbResult<()> {
    sqlx::query(
        r#"
        DELETE FROM "docbox_files_pages" AS "page"
        USING "docbox_files" AS "file"
        JOIN "docbox_folders" AS "folder" ON "file"."folder_id" = "folder"."id"
        WHERE "page"."file_id" = "file"."id" AND "folder"."document_box" = $1;
    "#,
    )
    .bind(scope)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn delete_file_pages_by_file_id(db: &DbPool, file_id: Uuid) -> DbResult<()> {
    sqlx::query(
        r#"
        DELETE FROM "docbox_files_pages" AS "page"
        WHERE "page"."file_id" = $1;
    "#,
    )
    .bind(file_id)
    .execute(db)
    .await?;
    Ok(())
}
