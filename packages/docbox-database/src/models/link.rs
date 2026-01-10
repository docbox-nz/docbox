use super::{
    document_box::DocumentBoxScopeRaw,
    folder::{FolderId, FolderPathSegment},
    user::{User, UserId},
};
use crate::{
    DbExecutor, DbResult,
    models::{
        folder::WithFullPath,
        shared::{CountResult, DocboxInputPair, WithFullPathScope},
    },
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{postgres::PgQueryResult, prelude::FromRow};
use utoipa::ToSchema;
use uuid::Uuid;

pub type LinkId = Uuid;

#[derive(Debug, Clone, FromRow, Serialize, sqlx::Type, ToSchema)]
#[sqlx(type_name = "docbox_link")]
pub struct Link {
    /// Unique identifier for the link
    #[schema(value_type = Uuid)]
    pub id: LinkId,
    /// Name of the link
    pub name: String,
    /// value of the link
    pub value: String,
    /// Whether the link is pinned
    pub pinned: bool,
    /// Parent folder ID
    #[schema(value_type = Uuid)]
    pub folder_id: FolderId,
    /// When the link was created
    pub created_at: DateTime<Utc>,
    /// User who created the link
    #[serde(skip)]
    pub created_by: Option<UserId>,
}

impl Eq for Link {}

impl PartialEq for Link {
    fn eq(&self, other: &Self) -> bool {
        self.id.eq(&other.id)
            && self.name.eq(&other.name)
            && self.value.eq(&other.value)
            && self.pinned.eq(&other.pinned)
            && self.folder_id.eq(&other.folder_id)
            && self.created_by.eq(&self.created_by)
            // Reduce precision when checking creation timestamp
            // (Database does not store the full precision)
            && self
                .created_at
                .timestamp_millis()
                .eq(&other.created_at.timestamp_millis())
    }
}

#[derive(Debug, FromRow, Serialize)]
pub struct LinkWithScope {
    #[sqlx(flatten)]
    pub link: Link,
    pub scope: String,
}

#[derive(Debug, Clone, Serialize, FromRow, ToSchema)]
pub struct LinkWithExtra {
    #[serde(flatten)]
    pub link: Link,
    /// User who created the link
    #[schema(nullable, value_type = User)]
    pub created_by: Option<User>,
    /// User who last modified the link
    #[schema(nullable, value_type = User)]
    pub last_modified_by: Option<User>,
    /// Last time the file was modified
    pub last_modified_at: Option<DateTime<Utc>>,
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
            pinned: false,
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

    pub async fn set_pinned(mut self, db: impl DbExecutor<'_>, pinned: bool) -> DbResult<Link> {
        sqlx::query(r#"UPDATE "docbox_links" SET "pinned" = $1 WHERE "id" = $2"#)
            .bind(pinned)
            .bind(self.id)
            .execute(db)
            .await?;

        self.pinned = pinned;
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
        sqlx::query_as(r#"SELECT "id", "name" FROM resolve_link_path($1)"#)
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
    pub async fn delete(&self, db: impl DbExecutor<'_>) -> DbResult<PgQueryResult> {
        sqlx::query(r#"DELETE FROM "docbox_links" WHERE "id" = $1"#)
            .bind(self.id)
            .execute(db)
            .await
    }

    /// Finds a collection of links that are within various document box scopes, resolves
    /// both the links themselves and the folder path to traverse to get to each link
    pub async fn resolve_with_extra_mixed_scopes(
        db: impl DbExecutor<'_>,
        links_scope_with_id: Vec<DocboxInputPair<'_>>,
    ) -> DbResult<Vec<WithFullPathScope<LinkWithExtra>>> {
        if links_scope_with_id.is_empty() {
            return Ok(Vec::new());
        }

        sqlx::query_as(r#"SELECT * FROM resolve_links_with_extra_mixed_scopes($1)"#)
            .bind(links_scope_with_id)
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

        sqlx::query_as(r#"SELECT * FROM resolve_links_with_extra($1, $2)"#)
            .bind(scope)
            .bind(link_ids)
            .fetch_all(db)
            .await
    }

    /// Finds all links within the provided parent folder
    pub async fn find_by_parent_with_extra(
        db: impl DbExecutor<'_>,
        parent_id: FolderId,
    ) -> DbResult<Vec<LinkWithExtra>> {
        sqlx::query_as(r#"SELECT * FROM resolve_links_by_parent_folder_with_extra($1)"#)
            .bind(parent_id)
            .fetch_all(db)
            .await
    }

    pub async fn find_with_extra(
        db: impl DbExecutor<'_>,
        scope: &DocumentBoxScopeRaw,
        link_id: LinkId,
    ) -> DbResult<Option<LinkWithExtra>> {
        sqlx::query_as(r#"SELECT * FROM resolve_link_by_id_with_extra($1, $2)"#)
            .bind(scope)
            .bind(link_id)
            .fetch_optional(db)
            .await
    }

    /// Get the total number of folders in the tenant
    pub async fn total_count(db: impl DbExecutor<'_>) -> DbResult<i64> {
        let count_result: CountResult =
            sqlx::query_as(r#"SELECT COUNT(*) AS "count" FROM "docbox_links""#)
                .fetch_one(db)
                .await?;

        Ok(count_result.count)
    }
}
