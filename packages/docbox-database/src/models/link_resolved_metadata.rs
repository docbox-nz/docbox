use crate::{DbExecutor, DbResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct LinkResolvedMetadata {
    /// URL of the resolved metadata
    pub url: String,
    /// The metadata itself
    #[sqlx(json)]
    pub metadata: StoredResolvedWebsiteMetadata,
    /// Timestamp of when the metadata will expire
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Deserialize)]
pub struct StoredResolvedWebsiteMetadata {
    pub title: Option<String>,
    pub og_title: Option<String>,
    pub og_description: Option<String>,
    pub og_image: Option<String>,
    pub best_favicon: Option<String>,
}

pub struct CreateLinkResolvedMetadata {
    pub url: String,
    pub metadata: StoredResolvedWebsiteMetadata,
    pub expires_at: DateTime<Utc>,
}

impl LinkResolvedMetadata {
    /// Create and insert a new resolved link metadata
    pub async fn create(
        db: impl DbExecutor<'_>,
        create: CreateLinkResolvedMetadata,
    ) -> DbResult<()> {
        let metadata = serde_json::to_value(&create.metadata)
            .map_err(|error| sqlx::Error::Encode(error.into()))?;

        sqlx::query(
            r#"
            INSERT INTO "docbox_links_resolved_metadata" ("url", "metadata", "expires_at")
            VALUES ($1, $2, $3)
            ON CONFLICT ("url") DO UPDATE
            SET
                "metadata" = EXCLUDED."metadata",
                "expires_at" = EXCLUDED."expires_at"
        "#,
        )
        .bind(create.url)
        .bind(metadata)
        .bind(create.expires_at)
        .execute(db)
        .await?;

        Ok(())
    }

    /// Query the resolved link metadata for the provided URL
    pub async fn query(
        db: impl DbExecutor<'_>,
        url: &str,
    ) -> DbResult<Option<LinkResolvedMetadata>> {
        sqlx::query_as(r#"SELECT * FROM "docbox_links_resolved_metadata" WHERE "url" = $1"#)
            .bind(url)
            .fetch_optional(db)
            .await
    }

    /// Deletes all metadata where the expiry date is less than `before`
    pub async fn delete_expired(
        db: impl DbExecutor<'_>,
        before: DateTime<Utc>,
    ) -> DbResult<Option<LinkResolvedMetadata>> {
        sqlx::query_as(r#"DELETE FROM "docbox_links_resolved_metadata" WHERE "expires_at" < $1"#)
            .bind(before)
            .fetch_optional(db)
            .await
    }
}
