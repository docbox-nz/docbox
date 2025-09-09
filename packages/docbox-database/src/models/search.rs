use serde::Serialize;
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
