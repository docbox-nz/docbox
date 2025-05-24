use super::{
    document_box::DocumentBoxScope,
    folder::{FolderId, FolderPathSegment},
    user::{User, UserId},
};
use crate::{DbExecutor, DbResult};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{postgres::PgRow, prelude::FromRow};
use utoipa::ToSchema;
use uuid::Uuid;

pub type LinkId = Uuid;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Link {
    /// Unique identifier for the file
    pub id: LinkId,
    /// Name of the link
    pub name: String,
    /// value of the link
    pub value: String,
    /// Parent folder ID
    pub folder_id: FolderId,
    /// When the file was created
    pub created_at: DateTime<Utc>,
    /// User who created the link
    pub created_by: Option<UserId>,
}

#[derive(Debug, FromRow, Serialize)]
pub struct LinkWithScope {
    #[sqlx(flatten)]
    pub link: Link,
    pub scope: String,
}

#[derive(Debug, Clone, Serialize, FromRow, ToSchema)]
pub struct LinkWithExtra {
    /// Unique identifier for the file
    pub id: Uuid,
    /// Name of the link
    pub name: String,
    /// value of the link
    pub value: String,
    /// Parent folder ID
    pub folder_id: Uuid,
    /// When the file was created
    pub created_at: DateTime<Utc>,
    /// User who created the file
    #[sqlx(flatten)]
    #[schema(nullable, value_type = User)]
    pub created_by: CreatedByUser,
    /// Last time the file was modified
    pub last_modified_at: Option<DateTime<Utc>>,
    /// User who last modified the file
    #[sqlx(flatten)]
    #[schema(nullable, value_type = User)]
    pub last_modified_by: LastModifiedByUser,
}

/// Wrapper type for extracting a [User] that was joined
/// from another table where the fields are prefixed with "cb_"
#[derive(Debug, Clone, Serialize)]
#[serde(transparent)]
pub struct CreatedByUser(pub Option<User>);

impl<'r> FromRow<'r, PgRow> for CreatedByUser {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;

        let id: Option<UserId> = row.try_get("cb_id")?;
        if let Some(id) = id {
            let name: Option<String> = row.try_get("cb_name")?;
            let image_id: Option<String> = row.try_get("cb_image_id")?;
            return Ok(CreatedByUser(Some(User { id, name, image_id })));
        }

        Ok(CreatedByUser(None))
    }
}

/// Wrapper type for extracting a [User] that was joined
/// from another table where the fields are prefixed with "lmb_"
#[derive(Debug, Clone, Serialize)]
#[serde(transparent)]
pub struct LastModifiedByUser(pub Option<User>);

impl<'r> FromRow<'r, PgRow> for LastModifiedByUser {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;

        let id: Option<UserId> = row.try_get("lmb_id")?;
        if let Some(id) = id {
            let name: Option<String> = row.try_get("lmb_name")?;
            let image_id: Option<String> = row.try_get("lmb_image_id")?;
            return Ok(LastModifiedByUser(Some(User { id, name, image_id })));
        }

        Ok(LastModifiedByUser(None))
    }
}

pub struct CreateLink {
    pub name: String,
    pub value: String,
    pub folder_id: FolderId,
    pub created_by: Option<UserId>,
}

impl Link {
    pub async fn create(
        db: impl DbExecutor<'_>,
        CreateLink {
            name,
            value,
            folder_id,
            created_by,
        }: CreateLink,
    ) -> anyhow::Result<Link> {
        let id = Uuid::new_v4();
        let created_at = Utc::now();

        sqlx::query(
            r#"INSERT INTO "docbox_links" (
                "id",
                "name",
                "value",
                "folder_id",
                "created_by",
                "created_at"
            ) VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(id)
        .bind(name.clone())
        .bind(value.clone())
        .bind(folder_id)
        .bind(created_by.as_ref())
        .bind(created_at)
        .execute(db)
        .await?;

        Ok(Link {
            id,
            name,
            value,
            folder_id,
            created_by,
            created_at,
        })
    }

    pub async fn move_to_folder(
        mut self,
        db: impl DbExecutor<'_>,
        folder_id: FolderId,
    ) -> DbResult<Link> {
        sqlx::query(r#"UPDATE "docbox_links" SET "folder_id" = $1 WHERE "id" = $2"#)
            .bind(folder_id)
            .bind(self.id)
            .execute(db)
            .await?;

        self.folder_id = folder_id;

        Ok(self)
    }

    pub async fn rename(mut self, db: impl DbExecutor<'_>, name: String) -> anyhow::Result<Link> {
        sqlx::query(r#"UPDATE "docbox_links" SET "name" = $1 WHERE "id" = $2"#)
            .bind(name.as_str())
            .bind(self.id)
            .execute(db)
            .await?;

        self.name = name.clone();
        Ok(self)
    }

    pub async fn update_value(
        mut self,
        db: impl DbExecutor<'_>,
        value: String,
    ) -> anyhow::Result<Link> {
        sqlx::query(r#"UPDATE "docbox_links" SET "value" = $1 WHERE "id" = $2"#)
            .bind(value.as_str())
            .bind(self.id)
            .execute(db)
            .await?;

        self.value = value.clone();

        Ok(self)
    }

    pub async fn all(
        db: impl DbExecutor<'_>,
        offset: u64,
        page_size: u64,
    ) -> DbResult<Vec<LinkWithScope>> {
        sqlx::query_as(
            r#"
            SELECT 
            "link".*,
            "folder"."document_box" AS "scope"
            FROM "docbox_links" AS "link"
            INNER JOIN "docbox_folders" "folder" ON "link"."folder_id" = "folder"."id"
            ORDER BY "link"."created_at" ASC
            OFFSET $1
            LIMIT $2
        "#,
        )
        .bind(offset as i64)
        .bind(page_size as i64)
        .fetch_all(db)
        .await
    }

    pub async fn find(
        db: impl DbExecutor<'_>,
        scope: &DocumentBoxScope,
        link_id: LinkId,
    ) -> DbResult<Option<Link>> {
        sqlx::query_as(
            r#"
            SELECT "link".*
            FROM "docbox_links" AS "link"
            INNER JOIN "docbox_folders" "folder" ON "link"."folder_id" = "folder"."id"
            WHERE "link"."id" = $1 AND "folder"."document_box" = $2
        "#,
        )
        .bind(link_id)
        .bind(scope)
        .fetch_optional(db)
        .await
    }
    /// Collects the IDs and names of all parent folders of the
    /// provided folder
    pub async fn resolve_path(
        db: impl DbExecutor<'_>,
        link_id: LinkId,
    ) -> DbResult<Vec<FolderPathSegment>> {
        sqlx::query_as(
            r#"
            WITH RECURSIVE "folder_hierarchy" AS (
                SELECT "id", "name", "folder_id", 0 AS "depth"
                FROM "docbox_links" 
                WHERE "docbox_links"."id" = $1 
                UNION ALL (
                    SELECT 
                        "folder"."id", 
                        "folder"."name", 
                        "folder"."folder_id", 
                        "folder_hierarchy"."depth" + 1 as "depth"
                    FROM "docbox_folders" AS "folder" 
                    INNER JOIN "folder_hierarchy" ON "folder"."id" = "folder_hierarchy"."folder_id"
                )
            ) 
            CYCLE "id" SET "looped" USING "traversal_path" 
            SELECT "folder_hierarchy"."id", "folder_hierarchy"."name" 
            FROM "folder_hierarchy" 
            WHERE "folder_hierarchy"."id" <> $1
            ORDER BY "folder_hierarchy"."depth" DESC
        "#,
        )
        .bind(link_id)
        .fetch_all(db)
        .await
    }

    /// Finds all links within the provided parent folder
    pub async fn find_by_parent(
        db: impl DbExecutor<'_>,
        parent_id: FolderId,
    ) -> DbResult<Vec<Link>> {
        sqlx::query_as(r#"SELECT * FROM "docbox_links" WHERE "folder_id" = $1"#)
            .bind(parent_id)
            .fetch_all(db)
            .await
    }

    /// Deletes the link
    pub async fn delete(&self, db: impl DbExecutor<'_>) -> DbResult<()> {
        sqlx::query(r#"DELETE FROM "docbox_links" WHERE "id" = $1"#)
            .bind(self.id)
            .execute(db)
            .await?;

        Ok(())
    }

    /// Finds all links within the provided parent folder
    pub async fn find_by_parent_with_extra(
        db: impl DbExecutor<'_>,
        parent_id: FolderId,
    ) -> DbResult<Vec<LinkWithExtra>> {
        sqlx::query_as(
            r#"
        SELECT 
            -- Link itself details
            "link".*,
            -- Creator user details
            "cu"."id" AS "cb_id", 
            "cu"."name" AS "cb_name", 
            "cu"."image_id" AS "cb_image_id", 
            -- Last modified date
            "ehl"."created_at" AS "last_modified_at", 
            -- Last modified user details
            "mu"."id" AS "lmb_id",  
            "mu"."name" AS "lmb_name", 
            "mu"."image_id" AS "lmb_image_id" 
        FROM "docbox_links" AS "link"
        -- Join on the creator
        LEFT JOIN "docbox_users" AS "cu" 
            ON "link"."created_by" = "cu"."id" 
        -- Join on the edit history (Latest only)
        LEFT JOIN LATERAL (
            -- Get the latest edit history entry
            SELECT "link_id", "user_id", "created_at" 
            FROM "docbox_edit_history"
            WHERE "link_id" = "link"."id" 
            ORDER BY "created_at" DESC 
            LIMIT 1
        ) AS "ehl" ON "link"."id" = "ehl"."link_id" 
        -- Join on the editor history latest edit user
        LEFT JOIN "docbox_users" AS "mu" ON "ehl"."user_id" = "mu"."id" 
        WHERE "link"."folder_id" = $1"#,
        )
        .bind(parent_id)
        .fetch_all(db)
        .await
    }

    pub async fn find_with_extra(
        db: impl DbExecutor<'_>,
        scope: &DocumentBoxScope,
        link_id: LinkId,
    ) -> DbResult<Option<LinkWithExtra>> {
        sqlx::query_as(
            r#"
        SELECT 
            -- Link itself details
            "link".*,
            -- Creator user details
            "cu"."id" AS "cb_id", 
            "cu"."name" AS "cb_name", 
            "cu"."image_id" AS "cb_image_id", 
            -- Last modified date
            "ehl"."created_at" AS "last_modified_at", 
            -- Last modified user details
            "mu"."id" AS "lmb_id",  
            "mu"."name" AS "lmb_name", 
            "mu"."image_id" AS "lmb_image_id" 
        FROM "docbox_links" AS "link"
        -- Join on the creator
        LEFT JOIN "docbox_users" AS "cu" 
            ON "link"."created_by" = "cu"."id" 
        -- Join on the parent folder
        INNER JOIN "docbox_folders" "folder" ON "link"."folder_id" = "folder"."id"
        -- Join on the edit history (Latest only)
        LEFT JOIN LATERAL (
            -- Get the latest edit history entry
            SELECT "link_id", "user_id", "created_at" 
            FROM "docbox_edit_history"
            WHERE "link_id" = "link"."id" 
            ORDER BY "created_at" DESC 
            LIMIT 1
        ) AS "ehl" ON "link"."id" = "ehl"."link_id" 
        -- Join on the editor history latest edit user
        LEFT JOIN "docbox_users" AS "mu" ON "ehl"."user_id" = "mu"."id" 
        WHERE "link"."id" = $1 AND "folder"."document_box" = $2"#,
        )
        .bind(link_id)
        .bind(scope)
        .fetch_optional(db)
        .await
    }
}
