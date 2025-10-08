use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{postgres::PgRow, prelude::FromRow};
use utoipa::ToSchema;
use uuid::Uuid;

use super::{
    document_box::DocumentBoxScopeRaw,
    folder::{FolderId, FolderPathSegment},
    user::{User, UserId},
};
use crate::{
    DbExecutor, DbResult,
    models::{
        document_box::DocumentBoxScopeRawRef,
        folder::{WithFullPath, WithFullPathScope},
        shared::TotalSizeResult,
    },
};

pub type FileId = Uuid;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct File {
    /// Unique identifier for the file
    pub id: FileId,
    /// Name of the file
    pub name: String,
    /// Mime type of the file content
    pub mime: String,
    /// Parent folder ID
    pub folder_id: FolderId,
    /// Hash of the file bytes stored in S3
    pub hash: String,
    /// Size of the file in bytes
    pub size: i32,
    /// Whether the file was determined to be encrypted when processing
    pub encrypted: bool,
    /// Whether the file is marked as pinned
    pub pinned: bool,
    /// S3 key pointing to the file
    #[serde(skip)]
    pub file_key: String,
    /// When the file was created
    pub created_at: DateTime<Utc>,
    /// User who created the file
    pub created_by: Option<UserId>,
    /// Optional parent file ID if the file is a child of
    /// some other file (i.e attachment for an email file)
    pub parent_id: Option<Uuid>,
}

#[derive(Debug, FromRow, Serialize)]
pub struct FileWithScope {
    #[sqlx(flatten)]
    pub file: File,
    pub scope: String,
}

/// File with the resolved creator and last modified data
#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct FileWithExtra {
    /// Unique identifier for the file
    #[schema(value_type = Uuid)]
    pub id: FileId,
    /// Name of the file
    pub name: String,
    /// Mime type of the file content
    pub mime: String,
    /// Parent folder ID
    #[schema(value_type = Uuid)]
    pub folder_id: FolderId,
    /// Hash of the file bytes stored in S3
    pub hash: String,
    /// Size of the file in bytes
    pub size: i32,
    /// Whether the file was determined to be encrypted when processing
    pub encrypted: bool,
    /// Whether the file is marked as pinned
    pub pinned: bool,
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
    /// Optional parent file if the file is a child
    #[schema(value_type = Option<Uuid>)]
    pub parent_id: Option<FileId>,
}

/// File with extra with an additional resolved full path
#[derive(Debug, FromRow, Serialize, ToSchema)]
pub struct ResolvedFileWithExtra {
    #[serde(flatten)]
    #[sqlx(flatten)]
    pub file: FileWithExtra,
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

#[derive(Debug, Serialize)]
pub struct FileIdWithScope {
    pub file_id: FileId,
    pub scope: DocumentBoxScopeRaw,
}

pub struct CreateFile {
    /// Fixed file ID to use instead of a randomly
    /// generated file ID
    pub fixed_id: Option<FileId>,
    /// Optional parent file if the file was created
    /// as the result of another file (i.e. email attachments)
    pub parent_id: Option<FileId>,

    pub name: String,
    pub mime: String,
    pub folder_id: FolderId,
    pub hash: String,
    pub size: i32,
    pub file_key: String,
    pub created_by: Option<UserId>,
    pub encrypted: bool,
}

impl File {
    pub async fn all(
        db: impl DbExecutor<'_>,
        offset: u64,
        page_size: u64,
    ) -> DbResult<Vec<FileWithScope>> {
        sqlx::query_as(
            r#"
            SELECT
            "file".*,
            "folder"."document_box" AS "scope"
            FROM "docbox_files" "file"
            INNER JOIN "docbox_folders" "folder" ON "file"."folder_id" = "folder"."id"
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

    pub async fn all_by_mime(
        db: impl DbExecutor<'_>,
        mime: &str,
        offset: u64,
        page_size: u64,
    ) -> DbResult<Vec<FileWithScope>> {
        sqlx::query_as(
            r#"
            SELECT
            "file".*,
            "folder"."document_box" AS "scope"
            FROM "docbox_files" "file"
            INNER JOIN "docbox_folders" "folder" ON "file"."folder_id" = "folder"."id"
            WHERE "file"."mime" = $1
            ORDER BY "created_at" ASC
            OFFSET $2
            LIMIT $3
        "#,
        )
        .bind(mime)
        .bind(offset as i64)
        .bind(page_size as i64)
        .fetch_all(db)
        .await
    }

    pub async fn move_to_folder(
        mut self,
        db: impl DbExecutor<'_>,
        folder_id: FolderId,
    ) -> DbResult<File> {
        sqlx::query(r#"UPDATE "docbox_files" SET "folder_id" = $1 WHERE "id" = $2"#)
            .bind(folder_id)
            .bind(self.id)
            .execute(db)
            .await?;

        self.folder_id = folder_id;

        Ok(self)
    }

    pub async fn rename(mut self, db: impl DbExecutor<'_>, name: String) -> DbResult<File> {
        sqlx::query(r#"UPDATE "docbox_files" SET "name" = $1 WHERE "id" = $2"#)
            .bind(name.as_str())
            .bind(self.id)
            .execute(db)
            .await?;

        self.name = name;

        Ok(self)
    }

    /// Updates the pinned state of the file
    pub async fn set_pinned(mut self, db: impl DbExecutor<'_>, pinned: bool) -> DbResult<File> {
        sqlx::query(r#"UPDATE "docbox_files" SET "pinned" = $1 WHERE "id" = $2"#)
            .bind(pinned)
            .bind(self.id)
            .execute(db)
            .await?;

        self.pinned = pinned;

        Ok(self)
    }

    /// Updates the encryption state of the file
    pub async fn set_encrypted(
        mut self,
        db: impl DbExecutor<'_>,
        encrypted: bool,
    ) -> DbResult<File> {
        sqlx::query(r#"UPDATE "docbox_files" SET "encrypted" = $1 WHERE "id" = $2"#)
            .bind(encrypted)
            .bind(self.id)
            .execute(db)
            .await?;

        self.encrypted = encrypted;

        Ok(self)
    }

    /// Updates the mime type of a file
    pub async fn set_mime(mut self, db: impl DbExecutor<'_>, mime: String) -> DbResult<File> {
        sqlx::query(r#"UPDATE "docbox_files" SET "mime" = $1 WHERE "id" = $2"#)
            .bind(&mime)
            .bind(self.id)
            .execute(db)
            .await?;

        self.mime = mime;

        Ok(self)
    }

    pub async fn create(
        db: impl DbExecutor<'_>,
        CreateFile {
            fixed_id,
            parent_id,
            name,
            mime,
            folder_id,
            hash,
            size,
            file_key,
            created_by,
            encrypted,
        }: CreateFile,
    ) -> DbResult<File> {
        let id = fixed_id.unwrap_or_else(Uuid::new_v4);
        let created_at = Utc::now();

        sqlx::query(
            r#"INSERT INTO "docbox_files" (
                    "id", "name", "mime", "folder_id", "hash", "size",
                    "encrypted", "file_key", "created_by", "created_at",
                    "parent_id"
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                "#,
        )
        .bind(id)
        .bind(name.as_str())
        .bind(mime.as_str())
        .bind(folder_id)
        .bind(hash.as_str())
        .bind(size)
        .bind(encrypted)
        .bind(file_key.as_str())
        .bind(created_by.as_ref())
        .bind(created_at)
        .bind(parent_id)
        .execute(db)
        .await?;

        Ok(File {
            id,
            name,
            mime,
            folder_id,
            hash,
            size,
            encrypted,
            file_key,
            created_by,
            created_at,
            parent_id,
            pinned: false,
        })
    }

    pub async fn all_convertable_paged(
        db: impl DbExecutor<'_>,
        offset: u64,
        page_size: u64,
        convertable_formats: Vec<&str>,
    ) -> DbResult<Vec<FileWithScope>> {
        sqlx::query_as(
            r#"
            SELECT
                "file".*,
                "folder"."document_box" AS "scope"
            FROM "docbox_files" AS "file"
            INNER JOIN "docbox_folders" "folder" ON "file"."folder_id" = "folder"."id"
            WHERE "mime" IS IN $1 AND "file"."encrypted" = FALSE
            ORDER BY "file"."created_at" ASC
            OFFSET $2
            LIMIT $3
        "#,
        )
        .bind(convertable_formats)
        .bind(offset as i64)
        .bind(page_size as i64)
        .fetch_all(db)
        .await
    }

    /// Finds a specific file using its full path scope -> folder -> file
    pub async fn find(
        db: impl DbExecutor<'_>,
        scope: &DocumentBoxScopeRaw,
        file_id: FileId,
    ) -> DbResult<Option<File>> {
        sqlx::query_as(
            r#"
            SELECT "file".*
            FROM "docbox_files" AS "file"
            INNER JOIN "docbox_folders" "folder" ON "file"."folder_id" = "folder"."id"
            WHERE "file"."id" = $1 AND "folder"."document_box" = $2
        "#,
        )
        .bind(file_id)
        .bind(scope)
        .fetch_optional(db)
        .await
    }

    /// Collects the IDs and names of all parent folders of the
    /// provided folder
    pub async fn resolve_path(
        db: impl DbExecutor<'_>,
        file_id: FileId,
    ) -> DbResult<Vec<FolderPathSegment>> {
        sqlx::query_as(
            r#"
            WITH RECURSIVE "folder_hierarchy" AS (
                SELECT "id", "name", "folder_id", 0 AS "depth"
                FROM "docbox_files"
                WHERE "docbox_files"."id" = $1
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
        .bind(file_id)
        .fetch_all(db)
        .await
    }

    pub async fn find_by_parent(
        db: impl DbExecutor<'_>,
        parent_id: FolderId,
    ) -> DbResult<Vec<File>> {
        sqlx::query_as(r#"SELECT * FROM "docbox_files" WHERE "folder_id" = $1"#)
            .bind(parent_id)
            .fetch_all(db)
            .await
    }

    /// Deletes the file
    pub async fn delete(&self, db: impl DbExecutor<'_>) -> DbResult<()> {
        sqlx::query(r#"DELETE FROM "docbox_files" WHERE "id" = $1"#)
            .bind(self.id)
            .execute(db)
            .await?;
        Ok(())
    }

    /// Finds a collection of files that are all within the same document box, resolves
    /// both the files themselves and the folder path to traverse to get to each file
    pub async fn resolve_with_extra(
        db: impl DbExecutor<'_>,
        scope: &DocumentBoxScopeRaw,
        file_ids: Vec<Uuid>,
    ) -> DbResult<Vec<WithFullPath<FileWithExtra>>> {
        if file_ids.is_empty() {
            return Ok(Vec::new());
        }

        sqlx::query_as(
            r#"
        -- Recursively resolve the file paths for each file creating a JSON array for the path
        WITH RECURSIVE "folder_hierarchy" AS (
            SELECT
                "f"."id" AS "file_id",
                "folder"."id" AS "folder_id",
                "folder"."name" AS "folder_name",
                "folder"."folder_id" AS "parent_folder_id",
                0 AS "depth",
                jsonb_build_array(jsonb_build_object('id', "folder"."id", 'name', "folder"."name")) AS "path"
            FROM "docbox_files" "f"
            JOIN "docbox_folders" "folder" ON "f"."folder_id" = "folder"."id"
            WHERE "f"."id" = ANY($1::uuid[]) AND "folder"."document_box" = $2
            UNION ALL
            SELECT
                "fh"."file_id",
                "parent"."id",
                "parent"."name",
                "parent"."folder_id",
                "fh"."depth" + 1,
                jsonb_build_array(jsonb_build_object('id', "parent"."id", 'name', "parent"."name")) || "fh"."path"
            FROM "folder_hierarchy" "fh"
            JOIN "docbox_folders" "parent" ON "fh"."parent_folder_id" = "parent"."id"
        ),
        "folder_paths" AS (
            SELECT "file_id", "path", ROW_NUMBER() OVER (PARTITION BY "file_id" ORDER BY "depth" DESC) AS "rn"
            FROM "folder_hierarchy"
        )
        SELECT
            -- File itself
            "file".*,
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
            -- File path from path lookup
            "fp"."path" AS "full_path"
        FROM "docbox_files" AS "file"
        -- Join on the creator
        LEFT JOIN "docbox_users" AS "cu"
            ON "file"."created_by" = "cu"."id"
        -- Join on the parent folder
        INNER JOIN "docbox_folders" "folder" ON "file"."folder_id" = "folder"."id"
        -- Join on the edit history (Latest only)
        LEFT JOIN (
            -- Get the latest edit history entry
            SELECT DISTINCT ON ("file_id") "file_id", "user_id", "created_at"
            FROM "docbox_edit_history"
            ORDER BY "file_id", "created_at" DESC
        ) AS "ehl" ON "file"."id" = "ehl"."file_id"
        -- Join on the editor history latest edit user
        LEFT JOIN "docbox_users" AS "mu" ON "ehl"."user_id" = "mu"."id"
        -- Join on the resolved folder path
        LEFT JOIN "folder_paths" "fp" ON "file".id = "fp"."file_id" AND "fp".rn = 1
        WHERE "file"."id" = ANY($1::uuid[]) AND "folder"."document_box" = $2"#,
        )
        .bind(file_ids)
        .bind(scope)
        .fetch_all(db)
        .await
    }

    /// Finds a collection of files that are within various document box scopes, resolves
    /// both the files themselves and the folder path to traverse to get to each file
    pub async fn resolve_with_extra_mixed_scopes(
        db: impl DbExecutor<'_>,
        files_scope_with_id: Vec<(DocumentBoxScopeRaw, FileId)>,
    ) -> DbResult<Vec<WithFullPathScope<FileWithExtra>>> {
        if files_scope_with_id.is_empty() {
            return Ok(Vec::new());
        }

        let (scopes, file_ids): (Vec<String>, Vec<FileId>) =
            files_scope_with_id.into_iter().unzip();

        sqlx::query_as(
            r#"
        -- Recursively resolve the file paths for each file creating a JSON array for the path
        WITH RECURSIVE
            "input_files" AS (
                SELECT file_id, document_box
                FROM UNNEST($1::text[], $2::uuid[]) AS t(document_box, file_id)
            ),
            "folder_hierarchy" AS (
                SELECT
                    "f"."id" AS "file_id",
                    "folder"."id" AS "folder_id",
                    "folder"."name" AS "folder_name",
                    "folder"."folder_id" AS "parent_folder_id",
                    0 AS "depth",
                    jsonb_build_array(jsonb_build_object('id', "folder"."id", 'name', "folder"."name")) AS "path"
                FROM "docbox_files" "f"
                JOIN "input_files" "i" ON "f"."id" = "i"."file_id"
                JOIN "docbox_folders" "folder" ON "f"."folder_id" = "folder"."id"
                WHERE "folder"."document_box" = "i"."document_box"
                UNION ALL
                SELECT
                    "fh"."file_id",
                    "parent"."id",
                    "parent"."name",
                    "parent"."folder_id",
                    "fh"."depth" + 1,
                    jsonb_build_array(jsonb_build_object('id', "parent"."id", 'name', "parent"."name")) || "fh"."path"
                FROM "folder_hierarchy" "fh"
                JOIN "docbox_folders" "parent" ON "fh"."parent_folder_id" = "parent"."id"
            ),
            "folder_paths" AS (
                SELECT "file_id", "path", ROW_NUMBER() OVER (PARTITION BY "file_id" ORDER BY "depth" DESC) AS "rn"
                FROM "folder_hierarchy"
            )
        SELECT
            -- File itself
            "file".*,
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
            -- File path from path lookup
            "fp"."path" AS "full_path",
            -- Include document box in response
            "folder"."document_box" AS "document_box"
        FROM "docbox_files" AS "file"
        -- Join on the creator
        LEFT JOIN "docbox_users" AS "cu"
            ON "file"."created_by" = "cu"."id"
        -- Join on the parent folder
        INNER JOIN "docbox_folders" "folder" ON "file"."folder_id" = "folder"."id"
        -- Join on the edit history (Latest only)
        LEFT JOIN (
            -- Get the latest edit history entry
            SELECT DISTINCT ON ("file_id") "file_id", "user_id", "created_at"
            FROM "docbox_edit_history"
            ORDER BY "file_id", "created_at" DESC
        ) AS "ehl" ON "file"."id" = "ehl"."file_id"
        -- Join on the editor history latest edit user
        LEFT JOIN "docbox_users" AS "mu" ON "ehl"."user_id" = "mu"."id"
        -- Join on the resolved folder path
        LEFT JOIN "folder_paths" "fp" ON "file".id = "fp"."file_id" AND "fp".rn = 1
        -- Join on the input files for filtering
        JOIN "input_files" "i" ON "file"."id" = "i"."file_id"
        -- Ensure correct document box
        WHERE "folder"."document_box" = "i"."document_box""#,
        )
        .bind(scopes)
        .bind(file_ids)
        .fetch_all(db)
        .await
    }

    /// Finds a specific file using its full path scope -> folder -> file
    /// fetching the additional details about the file like the creator and
    /// last modified
    pub async fn find_with_extra(
        db: impl DbExecutor<'_>,
        scope: &DocumentBoxScopeRaw,
        file_id: FileId,
    ) -> DbResult<Option<FileWithExtra>> {
        sqlx::query_as(
            r#"
        SELECT
            -- File itself
            "file".*,
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
        FROM "docbox_files" AS "file"
        -- Join on the creator
        LEFT JOIN "docbox_users" AS "cu"
            ON "file"."created_by" = "cu"."id"
        -- Join on the parent folder
        INNER JOIN "docbox_folders" "folder" ON "file"."folder_id" = "folder"."id"
        -- Join on the edit history (Latest only)
        LEFT JOIN (
            -- Get the latest edit history entry
            SELECT DISTINCT ON ("file_id") "file_id", "user_id", "created_at"
            FROM "docbox_edit_history"
            ORDER BY "file_id", "created_at" DESC
        ) AS "ehl" ON "file"."id" = "ehl"."file_id"
        -- Join on the editor history latest edit user
        LEFT JOIN "docbox_users" AS "mu" ON "ehl"."user_id" = "mu"."id"
        WHERE "file"."id" = $1 AND "folder"."document_box" = $2"#,
        )
        .bind(file_id)
        .bind(scope)
        .fetch_optional(db)
        .await
    }

    pub async fn find_by_parent_folder_with_extra(
        db: impl DbExecutor<'_>,
        parent_id: FolderId,
    ) -> DbResult<Vec<FileWithExtra>> {
        sqlx::query_as(
            r#"
        SELECT
            -- File itself
            "file".*,
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
        FROM "docbox_files" AS "file"
        -- Join on the creator
        LEFT JOIN "docbox_users" AS "cu"
            ON "file"."created_by" = "cu"."id"
        -- Join on the edit history (Latest only)
        LEFT JOIN (
            -- Get the latest edit history entry
            SELECT DISTINCT ON ("file_id") "file_id", "user_id", "created_at"
            FROM "docbox_edit_history"
            ORDER BY "file_id", "created_at" DESC
        ) AS "ehl" ON "file"."id" = "ehl"."file_id"
        -- Join on the editor history latest edit user
        LEFT JOIN "docbox_users" AS "mu" ON "ehl"."user_id" = "mu"."id"
        WHERE "file"."folder_id" = $1"#,
        )
        .bind(parent_id)
        .fetch_all(db)
        .await
    }

    pub async fn find_by_parent_file_with_extra(
        db: impl DbExecutor<'_>,
        parent_id: FileId,
    ) -> DbResult<Vec<FileWithExtra>> {
        sqlx::query_as(
            r#"
        SELECT
            -- File itself
            "file".*,
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
        FROM "docbox_files" AS "file"
        -- Join on the creator
        LEFT JOIN "docbox_users" AS "cu"
            ON "file"."created_by" = "cu"."id"
        -- Join on the edit history (Latest only)
        LEFT JOIN (
            -- Get the latest edit history entry
            SELECT DISTINCT ON ("file_id") "file_id", "user_id", "created_at"
            FROM "docbox_edit_history"
            ORDER BY "file_id", "created_at" DESC
        ) AS "ehl" ON "file"."id" = "ehl"."file_id"
        -- Join on the editor history latest edit user
        LEFT JOIN "docbox_users" AS "mu" ON "ehl"."user_id" = "mu"."id"
        WHERE "file"."parent_id" = $1"#,
        )
        .bind(parent_id)
        .fetch_all(db)
        .await
    }

    /// Get the total "size" of files within the current tenant, this does not include
    /// the size of generated files
    pub async fn total_size(db: impl DbExecutor<'_>) -> DbResult<i64> {
        let size_result: TotalSizeResult = sqlx::query_as(
            r#"
            SELECT COALESCE(SUM("file"."size"), 0) AS "total_size"
            FROM "docbox_files" "file";
        "#,
        )
        .fetch_one(db)
        .await?;

        Ok(size_result.total_size)
    }

    /// Get the total "size" of files within a specific scope, this does not include
    /// the size of generated files
    pub async fn total_size_within_scope(
        db: impl DbExecutor<'_>,
        scope: DocumentBoxScopeRawRef<'_>,
    ) -> DbResult<i64> {
        let size_result: TotalSizeResult = sqlx::query_as(
            r#"
            SELECT COALESCE(SUM("file"."size"), 0) AS "total_size"
            FROM "docbox_files" "file"
            INNER JOIN "docbox_folders" "folder" ON "file"."folder_id" = "folder"."id"
            WHERE "folder"."document_box" = $1;
        "#,
        )
        .bind(scope)
        .fetch_one(db)
        .await?;

        Ok(size_result.total_size)
    }
}
