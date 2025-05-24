use crate::{DbExecutor, DbResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use utoipa::ToSchema;

pub type DocumentBoxScope = String;

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct DocumentBox {
    /// Scope for the document box
    pub scope: DocumentBoxScope,
    /// Date of creation for the document box
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WithScope<T> {
    #[serde(flatten)]
    pub data: T,
    pub scope: DocumentBoxScope,
}

impl<T> WithScope<T> {
    pub fn new(data: T, scope: DocumentBoxScope) -> WithScope<T> {
        WithScope { data, scope }
    }
}

impl DocumentBox {
    /// Find all document boxes within a specific tenant
    pub async fn all(db: impl DbExecutor<'_>) -> DbResult<Vec<DocumentBox>> {
        sqlx::query_as(r#"SELECT * FROM "docbox_boxes""#)
            .fetch_all(db)
            .await
    }

    /// Find a specific document box by scope within a tenant
    pub async fn find_by_scope(
        db: impl DbExecutor<'_>,
        scope: &DocumentBoxScope,
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
    pub async fn delete(&self, db: impl DbExecutor<'_>) -> DbResult<()> {
        sqlx::query(r#"DELETE FROM "docbox_boxes" WHERE "scope" = $1"#)
            .bind(&self.scope)
            .execute(db)
            .await?;
        Ok(())
    }
}
