use crate::{DbExecutor, DbResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgQueryResult, prelude::FromRow};
use utoipa::ToSchema;

pub type DocumentBoxScopeRaw = String;
pub type DocumentBoxScopeRawRef<'a> = &'a str;

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct DocumentBox {
    /// Scope for the document box
    pub scope: DocumentBoxScopeRaw,
    /// Date of creation for the document box
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WithScope<T> {
    #[serde(flatten)]
    pub data: T,
    pub scope: DocumentBoxScopeRaw,
}

impl<T> WithScope<T> {
    pub fn new(data: T, scope: DocumentBoxScopeRaw) -> WithScope<T> {
        WithScope { data, scope }
    }
}

#[derive(FromRow)]
struct CountResult {
    count: i64,
}

impl DocumentBox {
    /// Get a page from the document boxes list
    pub async fn query(
        db: impl DbExecutor<'_>,
        offset: u64,
        limit: u64,
    ) -> DbResult<Vec<DocumentBox>> {
        sqlx::query_as(
            r#"
            SELECT * FROM "docbox_boxes"
            ORDER BY "created_at" DESC
            OFFSET $1 LIMIT $2"#,
        )
        .bind(offset as i64)
        .bind(limit as i64)
        .fetch_all(db)
        .await
    }

    /// Get the total number of document boxes in the tenant
    pub async fn total(db: impl DbExecutor<'_>) -> DbResult<i64> {
        let result: CountResult =
            sqlx::query_as(r#"SELECT COUNT(*) as "count" FROM "docbox_boxes""#)
                .fetch_one(db)
                .await?;

        Ok(result.count)
    }

    /// Get a page from the document boxes list based on a search query
    pub async fn search_query(
        db: impl DbExecutor<'_>,
        query: &str,
        offset: u64,
        limit: u64,
    ) -> DbResult<Vec<DocumentBox>> {
        sqlx::query_as(
            r#"
            SELECT * FROM "docbox_boxes"
            WHERE ($3 IS NULL OR "scope" ILIKE $3)
            ORDER BY "created_at" DESC
            OFFSET $1 LIMIT $2"#,
        )
        .bind(offset as i64)
        .bind(limit as i64)
        .bind(query)
        .fetch_all(db)
        .await
    }

    /// Get the total number of document boxes in the tenant for the specific search query
    pub async fn search_total(db: impl DbExecutor<'_>, query: &str) -> DbResult<i64> {
        let result: CountResult = sqlx::query_as(
            r#"
                SELECT COUNT(*) as "count" FROM "docbox_boxes"
                WHERE ($1 IS NULL OR "scope" ILIKE $1)
                "#,
        )
        .bind(query)
        .fetch_one(db)
        .await?;

        Ok(result.count)
    }

    /// Find a specific document box by scope within a tenant
    pub async fn find_by_scope(
        db: impl DbExecutor<'_>,
        scope: &DocumentBoxScopeRaw,
    ) -> DbResult<Option<DocumentBox>> {
        sqlx::query_as(r#"SELECT * FROM "docbox_boxes" WHERE "scope" = $1"#)
            .bind(scope)
            .fetch_optional(db)
            .await
    }

    pub async fn create(db: impl DbExecutor<'_>, scope: String) -> DbResult<DocumentBox> {
        let document_box = DocumentBox {
            scope,
            created_at: Utc::now(),
        };

        sqlx::query(r#"INSERT INTO "docbox_boxes" ("scope", "created_at") VALUES ($1, $2)"#)
            .bind(document_box.scope.as_str())
            .bind(document_box.created_at)
            .execute(db)
            .await?;

        Ok(document_box)
    }

    /// Deletes the document box
    pub async fn delete(&self, db: impl DbExecutor<'_>) -> DbResult<PgQueryResult> {
        sqlx::query(r#"DELETE FROM "docbox_boxes" WHERE "scope" = $1"#)
            .bind(&self.scope)
            .execute(db)
            .await
    }
}
