use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::{PgQueryResult, PgRow}, prelude::FromRow};
use tokio::try_join;
use utoipa::ToSchema;
use uuid::Uuid;

use super::{
    document_box::DocumentBoxScopeRaw,
    file::{File, FileWithExtra},
    link::{Link, LinkWithExtra},
    user::{User, UserId},
};
use crate::{DbExecutor, DbPool, DbResult};

pub type FolderId = Uuid;

/// Folder with all the children resolved
#[derive(Debug, Default, Serialize)]
pub struct ResolvedFolder {
    /// List of folders within the folder
    pub folders: Vec<Folder>,
    /// List of files within the folder
    pub files: Vec<File>,
    /// List of links within the folder
    pub links: Vec<Link>,
}

impl ResolvedFolder {
    pub async fn resolve(db: &DbPool, folder: &Folder) -> DbResult<ResolvedFolder> {
        let files_futures = File::find_by_parent(db, folder.id);
        let folders_future = Folder::find_by_parent(db, folder.id);
        let links_future = Link::find_by_parent(db, folder.id);

        let (files, folders, links) = try_join!(files_futures, folders_future, links_future)?;

        Ok(ResolvedFolder {
            folders,
            files,
            links,
        })
    }
}

/// Folder with all the children resolved, children also
/// resolve the user and last modified data
#[derive(Debug, Default, Serialize, ToSchema)]
pub struct ResolvedFolderWithExtra {
    /// Path to the resolved folder
    pub path: Vec<FolderPathSegment>,
    /// List of folders within the folder
    pub folders: Vec<FolderWithExtra>,
    /// List of files within the folder
    pub files: Vec<FileWithExtra>,
    /// List of links within the folder
    pub links: Vec<LinkWithExtra>,
}

impl ResolvedFolderWithExtra {
    pub async fn resolve(db: &DbPool, folder_id: FolderId) -> DbResult<ResolvedFolderWithExtra> {
        let path_future = Folder::resolve_path(db, folder_id);
        let files_futures = File::find_by_parent_folder_with_extra(db, folder_id);
        let folders_future = Folder::find_by_parent_with_extra(db, folder_id);
        let links_future = Link::find_by_parent_with_extra(db, folder_id);

        let (path, files, folders, links) =
            try_join!(path_future, files_futures, folders_future, links_future)?;

        Ok(ResolvedFolderWithExtra {
            path,
            folders,
            files,
            links,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FolderPathSegment {
    #[schema(value_type = Uuid)]
    pub id: FolderId,
    pub name: String,
}

impl<'r> FromRow<'r, PgRow> for FolderPathSegment {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;

        let id = row.try_get(0)?;
        let name = row.try_get(1)?;

        Ok(FolderPathSegment { id, name })
    }
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Folder {
    /// Unique identifier for the folder
    pub id: FolderId,
    /// Name of the file
    pub name: String,

    /// ID of the document box the folder belongs to
    pub document_box: DocumentBoxScopeRaw,
    /// Parent folder ID if the folder is a child
    pub folder_id: Option<FolderId>,

    /// When the file was created
    pub created_at: DateTime<Utc>,

    /// User who created the folder
    pub created_by: Option<UserId>,
}

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct FolderWithExtra {
    /// Unique identifier for the folder
    #[schema(value_type = Uuid)]
    pub id: FolderId,
    /// Name of the file
    pub name: String,

    /// Parent folder ID if the folder is a child
    #[schema(value_type = Option<Uuid>)]
    pub folder_id: Option<FolderId>,

    /// When the folder was created
    pub created_at: DateTime<Utc>,
    /// User who created the folder
    #[sqlx(flatten)]
    #[schema(nullable, value_type = User)]
    pub created_by: CreatedByUser,
    /// Last time the folder was modified
    pub last_modified_at: Option<DateTime<Utc>>,
    /// User who last modified the folder
    #[sqlx(flatten)]
    #[schema(nullable, value_type = User)]
    pub last_modified_by: LastModifiedByUser,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct WithFullPath<T> {
    #[serde(flatten)]
    #[sqlx(flatten)]
    pub data: T,
    #[sqlx(json)]
    pub full_path: Vec<FolderPathSegment>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct WithFullPathScope<T> {
    #[serde(flatten)]
    #[sqlx(flatten)]
    pub data: T,
    #[sqlx(json)]
    pub full_path: Vec<FolderPathSegment>, 
    pub document_box: DocumentBoxScopeRaw,
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
/// from another table where the fields are prefixed with "lmb_id"
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

pub struct CreateFolder {
    pub name: String,
    pub document_box: DocumentBoxScopeRaw,
    pub folder_id: Option<FolderId>,
    pub created_by: Option<UserId>,
}

#[derive(Debug, Serialize)]
pub struct FolderChildrenCount {
    pub file_count: i64,
    pub link_count: i64,
    pub folder_count: i64,
}

impl Folder {
    /// Collects the IDs of all child folders within the current folder
    ///
    /// Results are passed to the search engine when searching within a
    /// specific folder to only get results from the folder subtree
    pub async fn tree_all_children(&self, db: impl DbExecutor<'_>) -> DbResult<Vec<FolderId>> {
        #[derive(FromRow)]
        struct TempIdRow {
            id: FolderId,
        }

        let results: Vec<TempIdRow> = sqlx::query_as(
            r#"
        -- Recursively collect all child folders
        WITH RECURSIVE "folder_hierarchy" AS (
            SELECT "id", "folder_id"
            FROM "docbox_folders"
            WHERE "docbox_folders"."id" = $1 
            UNION ALL (
                SELECT
                    "folder"."id",
                    "folder"."folder_id"
                FROM "docbox_folders" AS "folder"
                INNER JOIN "folder_hierarchy" ON "folder"."folder_id" = "folder_hierarchy"."id"
            )
        )
        CYCLE "id" SET "looped" USING "traversal_path"
        SELECT "folder_hierarchy"."id" FROM "folder_hierarchy"
      "#,
        )
        .bind(self.id)
        .fetch_all(db)
        .await?;

        Ok(results.into_iter().map(|value| value.id).collect())
    }

    /// Uses a recursive query to count all the children in the provided
    /// folder
    pub async fn count_children(
        db: impl DbExecutor<'_>,
        folder_id: FolderId,
    ) -> DbResult<FolderChildrenCount> {
        let (file_count, link_count, folder_count): (i64, i64, i64) = sqlx::query_as(
            r#"
        -- Recursively collect all child folders
        WITH RECURSIVE "folder_hierarchy" AS (
            SELECT "id", "folder_id" 
            FROM "docbox_folders" 
            WHERE "docbox_folders"."id" = $1 
            UNION ALL (
                SELECT 
                    "folder"."id", 
                    "folder"."folder_id" 
                FROM "docbox_folders" AS "folder" 
                INNER JOIN "folder_hierarchy" ON "folder"."folder_id" = "folder_hierarchy"."id"
            )
        ) 
        CYCLE "id" SET "looped" USING "traversal_path" 
        SELECT * FROM (
            SELECT  
                -- Get counts of child tables
                COUNT(DISTINCT "file"."id") AS "file_count",
                COUNT(DISTINCT "link"."id") AS "link_count",
                COUNT(DISTINCT "folder"."id") AS "folder_count" 
            FROM "folder_hierarchy" 
            -- Join on collections of files, links and folders
            LEFT JOIN "docbox_files" AS "file" ON "file"."folder_id" = "folder_hierarchy"."id" 
            LEFT JOIN "docbox_links" AS "link" ON "link"."folder_id" = "folder_hierarchy"."id" 
            LEFT JOIN "docbox_folders" AS "folder" ON "folder"."folder_id" = "folder_hierarchy"."id"
        ) AS "counts"
        "#,
        )
        .bind(folder_id)
        .fetch_one(db)
        .await?;

        Ok(FolderChildrenCount {
            file_count,
            link_count,
            folder_count,
        })
    }

    /// Collects the IDs and names of all parent folders of the
    /// provided folder
    pub async fn resolve_path(
        db: impl DbExecutor<'_>,
        folder_id: FolderId,
    ) -> DbResult<Vec<FolderPathSegment>> {
        sqlx::query_as(
            r#"
            WITH RECURSIVE "folder_hierarchy" AS (
                SELECT "id", "name", "folder_id", 0 AS "depth"
                FROM "docbox_folders"
                WHERE "docbox_folders"."id" = $1 
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
        .bind(folder_id)
        .fetch_all(db)
        .await
    }

    pub async fn move_to_folder(
        mut self,
        db: impl DbExecutor<'_>,
        folder_id: FolderId,
    ) -> DbResult<Folder> {
        // Should never try moving a root folder
        debug_assert!(self.folder_id.is_some());

        sqlx::query(r#"UPDATE "docbox_folders" SET "folder_id" = $1 WHERE "id" = $2"#)
            .bind(folder_id)
            .bind(self.id)
            .execute(db)
            .await?;

        self.folder_id = Some(folder_id);

        Ok(self)
    }

    pub async fn rename(mut self, db: impl DbExecutor<'_>, name: String) -> DbResult<Folder> {
        sqlx::query(r#"UPDATE "docbox_folders" SET "name" = $1 WHERE "id" = $2"#)
            .bind(name.as_str())
            .bind(self.id)
            .execute(db)
            .await?;

        self.name = name;

        Ok(self)
    }

    pub async fn find_by_id(
        db: impl DbExecutor<'_>,
        scope: &DocumentBoxScopeRaw,
        id: FolderId,
    ) -> DbResult<Option<Folder>> {
        sqlx::query_as(r#"SELECT * FROM "docbox_folders" WHERE "id" = $1 AND "document_box" = $2"#)
            .bind(id)
            .bind(scope)
            .fetch_optional(db)
            .await
    }

    /// Get all folders and sub folder across any scope in a paginated fashion
    /// (Ignores roots of document boxes)
    pub async fn all_non_root(
        db: impl DbExecutor<'_>,
        offset: u64,
        page_size: u64,
    ) -> DbResult<Vec<Folder>> {
        sqlx::query_as(
            r#"
            SELECT * FROM "docbox_folders"
            WHERE "folder_id" IS NOT NULL 
            ORDER BY "created_at" ASC
            OFFSET $1
            LIMIT $2
        "#,
        )
        .bind(offset as i64)
        .bind(page_size as i64)
        .fetch_all(db)
        .await
    }

    pub async fn find_by_parent(
        db: impl DbExecutor<'_>,
        parent_id: FolderId,
    ) -> DbResult<Vec<Folder>> {
        sqlx::query_as(r#"SELECT * FROM "docbox_folders" WHERE "folder_id" = $1"#)
            .bind(parent_id)
            .fetch_all(db)
            .await
    }

    pub async fn find_root(
        db: impl DbExecutor<'_>,
        document_box: &DocumentBoxScopeRaw,
    ) -> DbResult<Option<Folder>> {
        sqlx::query_as(
            r#"SELECT * FROM "docbox_folders" WHERE "document_box" = $1 AND "folder_id" IS NULL"#,
        )
        .bind(document_box)
        .fetch_optional(db)
        .await
    }

    pub async fn create(
        db: impl DbExecutor<'_>,
        CreateFolder {
            name,
            document_box,
            folder_id,
            created_by,
        }: CreateFolder,
    ) -> DbResult<Folder> {
        let folder = Folder {
            id: Uuid::new_v4(),
            name,
            document_box,
            folder_id,
            created_by,
            created_at: Utc::now(),
        };

        sqlx::query(
            r#"
            INSERT INTO "docbox_folders" (
                "id", "name", "document_box", 
                "folder_id", "created_by", "created_at"
            ) 
            VALUES ($1, $2, $3, $4, $5, $6)
        "#,
        )
        .bind(folder.id)
        .bind(folder.name.as_str())
        .bind(folder.document_box.as_str())
        .bind(folder.folder_id)
        .bind(folder.created_by.as_ref())
        .bind(folder.created_at)
        .execute(db)
        .await?;

        Ok(folder)
    }

    /// Deletes the folder
    pub async fn delete(&self, db: impl DbExecutor<'_>) -> DbResult<PgQueryResult> {
        sqlx::query(r#"DELETE FROM "docbox_folders" WHERE "id" = $1"#)
            .bind(self.id)
            .execute(db)
            .await
    }

    /// Finds a collection of folders that are in various document box scopes, resolves
    /// both the folders themselves and the folder path to traverse to get to each folder
    pub async fn resolve_with_extra_mixed_scopes(
        db: impl DbExecutor<'_>,
        folders_scope_with_id: Vec<(DocumentBoxScopeRaw, FolderId)>,
    ) -> DbResult<Vec<WithFullPathScope<FolderWithExtra>>> {
        if folders_scope_with_id.is_empty() {
            return Ok(Vec::new());
        }

        let (scopes, folder_ids): (Vec<String>, Vec<FolderId>) =
            folders_scope_with_id.into_iter().unzip();

        sqlx::query_as(
            r#"
        -- Recursively resolve the folder paths for each folder creating a JSON array for the path
        WITH RECURSIVE 
            "input_folders" AS (
                SELECT folder_id, document_box
                FROM UNNEST($1::text[], $2::uuid[]) AS t(document_box, folder_id)
            ),
            "folder_hierarchy" AS (
                SELECT
                    "f"."id" AS "item_id",
                    "folder"."id" AS "folder_id",
                    "folder"."name" AS "folder_name",
                    "folder"."folder_id" AS "parent_folder_id",
                    0 AS "depth",
                    jsonb_build_array(jsonb_build_object('id', "folder"."id", 'name', "folder"."name")) AS "path"
                FROM "docbox_folders" "f"
                JOIN "input_folders" "i" ON "f"."id" = "i"."folder_id"
                JOIN "docbox_folders" "folder" ON "f"."folder_id" = "folder"."id"
                WHERE "folder"."document_box" = "i"."document_box"
                UNION ALL
                SELECT
                    "fh"."item_id",
                    "parent"."id",
                    "parent"."name",
                    "parent"."folder_id",
                    "fh"."depth" + 1,
                    jsonb_build_array(jsonb_build_object('id', "parent"."id", 'name', "parent"."name")) || "fh"."path"
                FROM "folder_hierarchy" "fh"
                JOIN "docbox_folders" "parent" ON "fh"."parent_folder_id" = "parent"."id"
            ),
            "folder_paths" AS (
                SELECT "item_id", "path", ROW_NUMBER() OVER (PARTITION BY "item_id" ORDER BY "depth" DESC) AS "rn"
                FROM "folder_hierarchy"
            )
        SELECT 
            -- folder itself 
            "folder".*,
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
            -- folder path from path lookup
            "fp"."path" AS "full_path" ,
            -- Include document box in response
            "folder"."document_box" AS "document_box"
        FROM "docbox_folders" AS "folder"
        -- Join on the creator
        LEFT JOIN "docbox_users" AS "cu" 
            ON "folder"."created_by" = "cu"."id" 
        -- Join on the edit history (Latest only)
        LEFT JOIN (
            -- Get the latest edit history entry
            SELECT DISTINCT ON ("folder_id") "folder_id", "user_id", "created_at" 
            FROM "docbox_edit_history"
            ORDER BY "folder_id", "created_at" DESC 
        ) AS "ehl" ON "folder"."id" = "ehl"."folder_id" 
        -- Join on the editor history latest edit user
        LEFT JOIN "docbox_users" AS "mu" ON "ehl"."user_id" = "mu"."id" 
        -- Join on the resolved folder path
        LEFT JOIN "folder_paths" "fp" ON "folder".id = "fp"."item_id" AND "fp".rn = 1
        -- Join on the input files for filtering
        JOIN "input_folders" "i" ON "folder"."id" = "i"."folder_id"
        -- Ensure correct document box
        WHERE "folder"."document_box" = "i"."document_box""#,
        )
        .bind(scopes)
        .bind(folder_ids)
        .fetch_all(db)
        .await
    }

    /// Finds a collection of folders that are all within the same document box, resolves
    /// both the folders themselves and the folder path to traverse to get to each folder
    pub async fn resolve_with_extra(
        db: impl DbExecutor<'_>,
        scope: &DocumentBoxScopeRaw,
        folder_ids: Vec<Uuid>,
    ) -> DbResult<Vec<WithFullPath<FolderWithExtra>>> {
        if folder_ids.is_empty() {
            return Ok(Vec::new());
        }

        sqlx::query_as(
        
            r#"
        -- Recursively resolve the folder paths for each folder creating a JSON array for the path
        WITH RECURSIVE "folder_hierarchy" AS (
            SELECT
                "f"."id" AS "item_id",
                "folder"."id" AS "folder_id",
                "folder"."name" AS "folder_name",
                "folder"."folder_id" AS "parent_folder_id",
                0 AS "depth",
                jsonb_build_array(jsonb_build_object('id', "folder"."id", 'name', "folder"."name")) AS "path"
            FROM "docbox_folders" "f"
            JOIN "docbox_folders" "folder" ON "f"."folder_id" = "folder"."id"
            WHERE "f"."id" = ANY($1::uuid[]) AND "folder"."document_box" = $2
            UNION ALL
            SELECT
                "fh"."item_id",
                "parent"."id",
                "parent"."name",
                "parent"."folder_id",
                "fh"."depth" + 1,
                jsonb_build_array(jsonb_build_object('id', "parent"."id", 'name', "parent"."name")) || "fh"."path"
            FROM "folder_hierarchy" "fh"
            JOIN "docbox_folders" "parent" ON "fh"."parent_folder_id" = "parent"."id"
        ),
        "folder_paths" AS (
            SELECT "item_id", "path", ROW_NUMBER() OVER (PARTITION BY "item_id" ORDER BY "depth" DESC) AS "rn"
            FROM "folder_hierarchy"
        )
        SELECT 
            -- folder itself 
            "folder".*,
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
            -- folder path from path lookup
            "fp"."path" AS "full_path" 
        FROM "docbox_folders" AS "folder"
        -- Join on the creator
        LEFT JOIN "docbox_users" AS "cu" 
            ON "folder"."created_by" = "cu"."id" 
        -- Join on the edit history (Latest only)
        LEFT JOIN (
            -- Get the latest edit history entry
            SELECT DISTINCT ON ("folder_id") "folder_id", "user_id", "created_at" 
            FROM "docbox_edit_history"
            ORDER BY "folder_id", "created_at" DESC 
        ) AS "ehl" ON "folder"."id" = "ehl"."folder_id" 
        -- Join on the editor history latest edit user
        LEFT JOIN "docbox_users" AS "mu" ON "ehl"."user_id" = "mu"."id" 
        -- Join on the resolved folder path
        LEFT JOIN "folder_paths" "fp" ON "folder".id = "fp"."item_id" AND "fp".rn = 1
        WHERE "folder"."id" = ANY($1::uuid[]) AND "folder"."document_box" = $2"#,
        )
        .bind(folder_ids)
        .bind(scope)
        .fetch_all(db)
        .await
    }

    pub async fn find_by_id_with_extra(
        db: impl DbExecutor<'_>,
        scope: &DocumentBoxScopeRaw,
        id: FolderId,
    ) -> DbResult<Option<FolderWithExtra>> {
        sqlx::query_as(
            r#"
        SELECT 
            -- Folder itself 
            "folder".*,
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
        FROM "docbox_folders" AS "folder"
        -- Join on the creator
        LEFT JOIN "docbox_users" AS "cu" 
            ON "folder"."created_by" = "cu"."id" 
        -- Join on the edit history (Latest only)
        LEFT JOIN (
            -- Get the latest edit history entry
            SELECT DISTINCT ON ("folder_id") "folder_id", "user_id", "created_at" 
            FROM "docbox_edit_history"
            ORDER BY "folder_id", "created_at" DESC 
        ) AS "ehl" ON "folder"."id" = "ehl"."folder_id" 
        -- Join on the editor history latest edit user
        LEFT JOIN "docbox_users" AS "mu" ON "ehl"."user_id" = "mu"."id" 
        WHERE "folder"."id" = $1 AND "folder"."document_box" = $2"#,
        )
        .bind(id)
        .bind(scope)
        .fetch_optional(db)
        .await
    }

    pub async fn find_by_parent_with_extra(
        db: impl DbExecutor<'_>,
        parent_id: FolderId,
    ) -> DbResult<Vec<FolderWithExtra>> {
        sqlx::query_as(
            r#"
        SELECT 
            -- Folder itself 
            "folder".*,
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
        FROM "docbox_folders" AS "folder"
        -- Join on the creator
        LEFT JOIN "docbox_users" AS "cu" 
            ON "folder"."created_by" = "cu"."id" 
        -- Join on the edit history (Latest only)
        LEFT JOIN (
            -- Get the latest edit history entry
            SELECT DISTINCT ON ("folder_id") "folder_id", "user_id", "created_at" 
            FROM "docbox_edit_history"
            ORDER BY "folder_id", "created_at" DESC
        ) AS "ehl" ON "folder"."id" = "ehl"."folder_id" 
        -- Join on the editor history latest edit user
        LEFT JOIN "docbox_users" AS "mu" ON "ehl"."user_id" = "mu"."id" 
        WHERE "folder"."folder_id" = $1"#,
        )
        .bind(parent_id)
        .fetch_all(db)
        .await
    }

    pub async fn find_root_with_extra(
        db: impl DbExecutor<'_>,
        document_box: &DocumentBoxScopeRaw,
    ) -> DbResult<Option<FolderWithExtra>> {
        sqlx::query_as(
            r#"
        SELECT 
            -- Folder itself 
            "folder".*,
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
        FROM "docbox_folders" AS "folder"
        -- Join on the creator
        LEFT JOIN "docbox_users" AS "cu" 
            ON "folder"."created_by" = "cu"."id" 
        -- Join on the edit history (Latest only)
        LEFT JOIN (
            -- Get the latest edit history entry
            SELECT DISTINCT ON ("folder_id") "folder_id", "user_id", "created_at" 
            FROM "docbox_edit_history"
            ORDER BY "folder_id", "created_at" DESC 
        ) AS "ehl" ON "folder"."id" = "ehl"."folder_id" 
        -- Join on the editor history latest edit user
        LEFT JOIN "docbox_users" AS "mu" ON "ehl"."user_id" = "mu"."id" 
        WHERE "folder"."document_box" = $1 AND "folder"."folder_id" IS NULL"#,
        )
        .bind(document_box)
        .fetch_optional(db)
        .await
    }
}
