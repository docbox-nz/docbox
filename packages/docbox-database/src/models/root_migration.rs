use crate::{DbExecutor, DbResult};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::prelude::FromRow;

/// Structure for tracking migrations applied to the root
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct RootMigration {
    pub name: String,
    pub applied_at: DateTime<Utc>,
}

pub struct CreateRootMigration {
    pub name: String,
    pub applied_at: DateTime<Utc>,
}

impl RootMigration {
    /// Create a new tenant migration
    pub async fn create(db: impl DbExecutor<'_>, create: CreateRootMigration) -> DbResult<()> {
        sqlx::query(
            r#"
            INSERT INTO "docbox_root_migrations" ("name", "applied_at")
            VALUES ($1, $2)
        "#,
        )
        .bind(create.name)
        .bind(create.applied_at)
        .execute(db)
        .await?;

        Ok(())
    }

    /// Find all applied migrations
    pub async fn all(db: impl DbExecutor<'_>) -> DbResult<Vec<RootMigration>> {
        sqlx::query_as(r#"SELECT * FROM "docbox_root_migrations""#)
            .fetch_all(db)
            .await
    }
}
