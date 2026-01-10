use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{postgres::PgQueryResult, prelude::FromRow};
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
        folder::WithFullPath,
        shared::{CountResult, DocboxInputPair, TotalSizeResult, WithFullPathScope},
    },
};

pub type FileId = Uuid;

#[derive(Debug, Clone, FromRow, Serialize, sqlx::Type, ToSchema)]
#[sqlx(type_name = "docbox_file")]
pub struct File {
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
    /// Optional parent file ID if the file is a child of
    /// some other file (i.e attachment for an email file)
    #[schema(value_type = Option<Uuid>)]
    pub parent_id: Option<FileId>,
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
    #[serde(skip)]
    pub created_by: Option<UserId>,
}

impl Eq for File {}

impl PartialEq for File {
    fn eq(&self, other: &Self) -> bool {
        self.id.eq(&other.id)
            && self.name.eq(&other.name)
            && self.mime.eq(&other.mime)
            && self.folder_id.eq(&other.folder_id)
            && self.parent_id.eq(&other.parent_id)
            && self.hash.eq(&other.hash)
            && self.size.eq(&other.size)
            && self.encrypted.eq(&other.encrypted)
            && self.pinned.eq(&other.pinned)
            && self.file_key.eq(&other.file_key)
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
pub struct FileWithScope {
    #[sqlx(flatten)]
    pub file: File,
    pub scope: String,
}

/// File with the resolved creator and last modified data
#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct FileWithExtra {
    #[serde(flatten)]
    pub file: File,
    #[schema(nullable, value_type = User)]
    pub created_by: Option<User>,
    #[schema(nullable, value_type = User)]
    pub last_modified_by: Option<User>,
    /// Last time the file was modified
    pub last_modified_at: Option<DateTime<Utc>>,
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

#[derive(Debug, Default)]
pub struct CreateFile {
    /// ID for the file to use
    pub id: FileId,

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
    pub created_at: DateTime<Utc>,
    pub encrypted: bool,
}

impl File {
    pub async fn create(
        db: impl DbExecutor<'_>,
        CreateFile {
            id,
            parent_id,
            name,
            mime,
            folder_id,
            hash,
            size,
            file_key,
            created_by,
            created_at,
            encrypted,
        }: CreateFile,
    ) -> DbResult<File> {
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
        sqlx::query_as(r#"SELECT "id", "name" FROM resolve_file_path($1)"#)
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
    pub async fn delete(&self, db: impl DbExecutor<'_>) -> DbResult<PgQueryResult> {
        sqlx::query(r#"DELETE FROM "docbox_files" WHERE "id" = $1"#)
            .bind(self.id)
            .execute(db)
            .await
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

        sqlx::query_as(r#"SELECT * FROM resolve_files_with_extra($1, $2)"#)
            .bind(scope)
            .bind(file_ids)
            .fetch_all(db)
            .await
    }

    /// Finds a collection of files that are within various document box scopes, resolves
    /// both the files themselves and the folder path to traverse to get to each file
    pub async fn resolve_with_extra_mixed_scopes(
        db: impl DbExecutor<'_>,
        files_scope_with_id: Vec<DocboxInputPair<'_>>,
    ) -> DbResult<Vec<WithFullPathScope<FileWithExtra>>> {
        if files_scope_with_id.is_empty() {
            return Ok(Vec::new());
        }

        sqlx::query_as(
            r#"SELECT * FROM resolve_files_with_extra_mixed_scopes($1::docbox_input_pair[])"#,
        )
        .bind(files_scope_with_id)
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
        sqlx::query_as(r#"SELECT * FROM resolve_file_by_id_with_extra($1, $2)"#)
            .bind(scope)
            .bind(file_id)
            .fetch_optional(db)
            .await
    }

    pub async fn find_by_parent_folder_with_extra(
        db: impl DbExecutor<'_>,
        parent_id: FolderId,
    ) -> DbResult<Vec<FileWithExtra>> {
        sqlx::query_as(r#"SELECT * FROM resolve_files_by_parent_folder_with_extra($1)"#)
            .bind(parent_id)
            .fetch_all(db)
            .await
    }

    pub async fn find_by_parent_file_with_extra(
        db: impl DbExecutor<'_>,
        parent_id: FileId,
    ) -> DbResult<Vec<FileWithExtra>> {
        sqlx::query_as(r#"SELECT * FROM resolve_files_by_parent_file_with_extra($1)"#)
            .bind(parent_id)
            .fetch_all(db)
            .await
    }

    /// Get the total number of files in the tenant
    pub async fn total_count(db: impl DbExecutor<'_>) -> DbResult<i64> {
        let count_result: CountResult =
            sqlx::query_as(r#"SELECT COUNT(*) AS "count" FROM "docbox_files""#)
                .fetch_one(db)
                .await?;

        Ok(count_result.count)
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
