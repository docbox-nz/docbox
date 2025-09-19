use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use uuid::Uuid;

use crate::{DbPool, DbResult, models::document_box::DocumentBoxScopeRaw};

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

pub async fn count_search_file_pages(
    db: &DbPool,
    scope: &DocumentBoxScopeRaw,
    file_id: Uuid,
    query: &str,
) -> DbResult<DbPageCountResult> {
    sqlx::query_as(
        r#"
        SELECT COUNT(*) AS "count"
        FROM "docbox_folders" "folder"
        JOIN "docbox_files" "file" ON "file"."folder_id" = "folder"."id"
        JOIN "docbox_files_pages" "page" ON "page"."file_id" = "file"."id"
        WHERE "folder"."document_box" = $1
            AND "file"."id" = $2
            AND "page"."file_id" = $2
            AND "page"."content_tsv" @@ plainto_tsquery('english', $3)
    "#,
    )
    .bind(scope)
    .bind(file_id)
    .bind(query)
    .fetch_one(db)
    .await
}

pub async fn search_file_pages(
    db: &DbPool,
    scope: &DocumentBoxScopeRaw,
    file_id: Uuid,
    query: &str,
    limit: i64,
    offset: i64,
) -> DbResult<Vec<DbPageResult>> {
    sqlx::query_as(r#"
        SELECT
            "page"."page",
            ts_headline('english', "page"."content", plainto_tsquery('english', $3)) AS "highlighted_content",
            ts_rank("page"."content_tsv", plainto_tsquery('english', $3)) AS "rank"
        FROM "docbox_folders" "folder"
        JOIN "docbox_files" "file" ON "file"."folder_id" = "folder"."id"
        JOIN "docbox_files_pages" "page" ON "page"."file_id" = "file"."id"
        WHERE "folder"."document_box" = $1
            AND "file"."id" = $2
            AND "page"."file_id" = $2
            AND "page"."content_tsv" @@ plainto_tsquery('english', $3)
        ORDER BY "rank" DESC
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

pub async fn delete_file_pages_by_scope(db: &DbPool, scope: &DocumentBoxScopeRaw) -> DbResult<()> {
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

#[derive(Debug, Deserialize)]
pub struct DbSearchPageResult {
    pub page: i64,
    pub matched: String,
}

#[derive(Debug, FromRow)]
pub struct DbSearchResult {
    pub item_type: String,
    pub item_id: Uuid,
    pub document_box: DocumentBoxScopeRaw,
    pub name_match_tsv: bool,
    pub name_match: bool,
    pub content_match: bool,
    pub total_hits: i64,
    #[sqlx(json)]
    pub page_matches: Vec<DbSearchPageResult>,
    pub total_count: i64,
    pub rank: f64,
}
