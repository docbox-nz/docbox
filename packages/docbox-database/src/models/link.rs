use super::{
    document_box::DocumentBoxScopeRaw,
    folder::{FolderId, FolderPathSegment},
    user::{User, UserId},
};
use crate::{
    DbExecutor, DbResult,
    models::folder::{WithFullPath, WithFullPathScope},
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{postgres::PgRow, prelude::FromRow};
use utoipa::ToSchema;
use uuid::Uuid;

pub type LinkId = Uuid;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Link {
    /// Unique identifier for the link
    pub id: LinkId,
    /// Name of the link
    pub name: String,
    /// value of the link
    pub value: String,
    /// Parent folder ID
    pub folder_id: FolderId,
    /// When the link was created
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
    /// Unique identifier for the link
    pub id: Uuid,
    /// Name of the link
    pub name: String,
    /// value of the link
    pub value: String,
    /// Parent folder ID
    pub folder_id: Uuid,
    /// When the link was created
    pub created_at: DateTime<Utc>,
    /// User who created the link
    #[sqlx(flatten)]
    #[schema(nullable, value_type = User)]
    pub created_by: CreatedByUser,
    /// Last time the link was modified
    pub last_modified_at: Option<DateTime<Utc>>,
    /// User who last modified the link
    #[sqlx(flatten)]
    #[schema(nullable, value_type = User)]
    pub last_modified_by: LastModifiedByUser,
}

/// Link with extra with an additional resolved full path
#[derive(Debug, FromRow, Serialize, ToSchema)]
pub struct ResolvedLinkWithExtra {
    #[serde(flatten)]
    #[sqlx(flatten)]
    pub link: LinkWithExtra,
    #[sqlx(json)]
    pub full_path: Vec<FolderPathSegment>,
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
    ) -> DbResult<Link> {
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

    pub async fn rename(mut self, db: impl DbExecutor<'_>, name: String) -> DbResult<Link> {
        sqlx::query(r#"UPDATE "docbox_links" SET "name" = $1 WHERE "id" = $2"#)
            .bind(name.as_str())
            .bind(self.id)
            .execute(db)
            .await?;

        self.name = name;
        Ok(self)
    }

    pub async fn update_value(mut self, db: impl DbExecutor<'_>, value: String) -> DbResult<Link> {
        sqlx::query(r#"UPDATE "docbox_links" SET "value" = $1 WHERE "id" = $2"#)
            .bind(value.as_str())
            .bind(self.id)
            .execute(db)
            .await?;

        self.value = value;

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
        scope: &DocumentBoxScopeRaw,
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

    /// Finds a collection of links that are within various document box scopes, resolves
    /// both the links themselves and the folder path to traverse to get to each link
    pub async fn resolve_with_extra_mixed_scopes(
        db: impl DbExecutor<'_>,
        links_scope_with_id: Vec<(DocumentBoxScopeRaw, LinkId)>,
    ) -> DbResult<Vec<WithFullPathScope<LinkWithExtra>>> {
        if links_scope_with_id.is_empty() {
            return Ok(Vec::new());
        }

        let (scopes, link_ids): (Vec<String>, Vec<LinkId>) =
            links_scope_with_id.into_iter().unzip();

        sqlx::query_as(
            r#"
        -- Recursively resolve the link paths for each link creating a JSON array for the path
        WITH RECURSIVE 
            "input_links" AS (
                SELECT link_id, document_box
                FROM UNNEST($1::text[], $2::uuid[]) AS t(document_box, link_id)
            ),
            "folder_hierarchy" AS (
                SELECT
                    "f"."id" AS "link_id",
                    "folder"."id" AS "folder_id",
                    "folder"."name" AS "folder_name",
                    "folder"."folder_id" AS "parent_folder_id",
                    0 AS "depth",
                    jsonb_build_array(jsonb_build_object('id', "folder"."id", 'name', "folder"."name")) AS "path"
                FROM "docbox_links" "f"
                JOIN "input_links" "i" ON "f"."id" = "i"."link_id"
                JOIN "docbox_folders" "folder" ON "f"."folder_id" = "folder"."id"
                WHERE "folder"."document_box" = "i"."document_box"
                UNION ALL
                SELECT
                    "fh"."link_id",
                    "parent"."id",
                    "parent"."name",
                    "parent"."folder_id",
                    "fh"."depth" + 1,
                    jsonb_build_array(jsonb_build_object('id', "parent"."id", 'name', "parent"."name")) || "fh"."path"
                FROM "folder_hierarchy" "fh"
                JOIN "docbox_folders" "parent" ON "fh"."parent_folder_id" = "parent"."id"
            ),
            "folder_paths" AS (
                SELECT "link_id", "path", ROW_NUMBER() OVER (PARTITION BY "link_id" ORDER BY "depth" DESC) AS "rn"
                FROM "folder_hierarchy"
            )
        SELECT 
            -- link itself 
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
            "mu"."image_id" AS "lmb_image_id" ,
            -- link path from path lookup
            "fp"."path" AS "full_path",
            -- Include document box in response
            "folder"."document_box" AS "document_box" 
        FROM "docbox_links" AS "link"
        -- Join on the creator
        LEFT JOIN "docbox_users" AS "cu" 
            ON "link"."created_by" = "cu"."id" 
        -- Join on the parent folder
        INNER JOIN "docbox_folders" "folder" ON "link"."folder_id" = "folder"."id"
        -- Join on the edit history (Latest only)
        LEFT JOIN (
            -- Get the latest edit history entry
            SELECT DISTINCT ON ("link_id") "link_id", "user_id", "created_at" 
            FROM "docbox_edit_history"
            ORDER BY "link_id", "created_at" DESC 
        ) AS "ehl" ON "link"."id" = "ehl"."link_id" 
        -- Join on the editor history latest edit user
        LEFT JOIN "docbox_users" AS "mu" ON "ehl"."user_id" = "mu"."id" 
        -- Join on the resolved folder path
        LEFT JOIN "folder_paths" "fp" ON "link".id = "fp"."link_id" AND "fp".rn = 1
        -- Join on the input files for filtering
        JOIN "input_links" "i" ON "link"."id" = "i"."link_id"
        -- Ensure correct document box
        WHERE "folder"."document_box" = "i"."document_box""#,
        )
        .bind(scopes)
        .bind(link_ids)
        .fetch_all(db)
        .await
    }

    /// Finds a collection of links that are all within the same document box, resolves
    /// both the links themselves and the folder path to traverse to get to each link
    pub async fn resolve_with_extra(
        db: impl DbExecutor<'_>,
        scope: &DocumentBoxScopeRaw,
        link_ids: Vec<Uuid>,
    ) -> DbResult<Vec<WithFullPath<LinkWithExtra>>> {
        if link_ids.is_empty() {
            return Ok(Vec::new());
        }

        sqlx::query_as(
            r#"
        -- Recursively resolve the link paths for each link creating a JSON array for the path
        WITH RECURSIVE "folder_hierarchy" AS (
            SELECT
                "f"."id" AS "link_id",
                "folder"."id" AS "folder_id",
                "folder"."name" AS "folder_name",
                "folder"."folder_id" AS "parent_folder_id",
                0 AS "depth",
                jsonb_build_array(jsonb_build_object('id', "folder"."id", 'name', "folder"."name")) AS "path"
            FROM "docbox_links" "f"
            JOIN "docbox_folders" "folder" ON "f"."folder_id" = "folder"."id"
            WHERE "f"."id" = ANY($1::uuid[]) AND "folder"."document_box" = $2
            UNION ALL
            SELECT
                "fh"."link_id",
                "parent"."id",
                "parent"."name",
                "parent"."folder_id",
                "fh"."depth" + 1,
                jsonb_build_array(jsonb_build_object('id', "parent"."id", 'name', "parent"."name")) || "fh"."path"
            FROM "folder_hierarchy" "fh"
            JOIN "docbox_folders" "parent" ON "fh"."parent_folder_id" = "parent"."id"
        ),
        "folder_paths" AS (
            SELECT "link_id", "path", ROW_NUMBER() OVER (PARTITION BY "link_id" ORDER BY "depth" DESC) AS "rn"
            FROM "folder_hierarchy"
        )
        SELECT 
            -- link itself 
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
            "mu"."image_id" AS "lmb_image_id" ,
            -- link path from path lookup
            "fp"."path" AS "full_path" 
        FROM "docbox_links" AS "link"
        -- Join on the creator
        LEFT JOIN "docbox_users" AS "cu" 
            ON "link"."created_by" = "cu"."id" 
        -- Join on the parent folder
        INNER JOIN "docbox_folders" "folder" ON "link"."folder_id" = "folder"."id"
        -- Join on the edit history (Latest only)
        LEFT JOIN (
            -- Get the latest edit history entry
            SELECT DISTINCT ON ("link_id") "link_id", "user_id", "created_at" 
            FROM "docbox_edit_history"
            ORDER BY "link_id", "created_at" DESC 
        ) AS "ehl" ON "link"."id" = "ehl"."link_id" 
        -- Join on the editor history latest edit user
        LEFT JOIN "docbox_users" AS "mu" ON "ehl"."user_id" = "mu"."id" 
        -- Join on the resolved folder path
        LEFT JOIN "folder_paths" "fp" ON "link".id = "fp"."link_id" AND "fp".rn = 1
        WHERE "link"."id" = ANY($1::uuid[]) AND "folder"."document_box" = $2"#,
        )
        .bind(link_ids)
        .bind(scope)
        .fetch_all(db)
        .await
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
            SELECT DISTINCT ON ("link_id") "link_id", "user_id", "created_at" 
            FROM "docbox_edit_history"
            ORDER BY "link_id", "created_at" DESC 
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
        scope: &DocumentBoxScopeRaw,
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
        LEFT JOIN (
            -- Get the latest edit history entry
            SELECT DISTINCT ON ("link_id") "link_id", "user_id", "created_at" 
            FROM "docbox_edit_history"
            ORDER BY "link_id", "created_at" DESC 
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
