use super::{
    document_box::DocumentBoxScopeRaw,
    file::{File, FileWithExtra},
    link::{Link, LinkWithExtra},
    user::{User, UserId},
};
use crate::{
    DbExecutor, DbPool, DbResult,
    models::shared::{CountResult, DocboxInputPair, FolderPathSegment, WithFullPath},
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{postgres::PgQueryResult, prelude::FromRow};
use tokio::try_join;
use utoipa::ToSchema;
use uuid::Uuid;

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

#[derive(Debug, Clone, Serialize, ToSchema, FromRow, sqlx::Type)]
#[sqlx(type_name = "docbox_folder")]
pub struct Folder {
    /// Unique identifier for the folder
    #[schema(value_type = Uuid)]
    pub id: FolderId,
    /// Name of the file
    pub name: String,

    /// Whether the folder is marked as pinned
    pub pinned: bool,

    /// ID of the document box the folder belongs to
    pub document_box: DocumentBoxScopeRaw,
    /// Parent folder ID if the folder is a child
    #[schema(value_type = Option<Uuid>)]
    pub folder_id: Option<FolderId>,

    /// When the folder was created
    pub created_at: DateTime<Utc>,
    /// User who created the folder
    #[serde(skip)]
    pub created_by: Option<UserId>,
}

impl Eq for Folder {}

impl PartialEq for Folder {
    fn eq(&self, other: &Self) -> bool {
        self.id.eq(&other.id)
            && self.name.eq(&other.name)
            && self.pinned.eq(&other.pinned)
            && self.document_box.eq(&other.document_box)
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

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct FolderWithExtra {
    #[serde(flatten)]
    pub folder: Folder,
    #[schema(nullable, value_type = User)]
    pub created_by: Option<User>,
    #[schema(nullable, value_type = User)]
    pub last_modified_by: Option<User>,
    /// Last time the folder was modified
    pub last_modified_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default)]
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
            pinned: false,
        };

        sqlx::query(
            r#"
            INSERT INTO "docbox_folders" (
                "id", "name", "document_box",  "folder_id",
                "created_by", "created_at"
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
        .bind(folder.pinned)
        .execute(db)
        .await?;

        Ok(folder)
    }

    /// Collects the IDs of all child folders within the current folder
    ///
    /// Results are passed to the search engine when searching within a
    /// specific folder to only get results from the folder subtree
    pub async fn tree_all_children(&self, db: impl DbExecutor<'_>) -> DbResult<Vec<FolderId>> {
        #[derive(FromRow)]
        struct TempIdRow {
            id: FolderId,
        }

        let results: Vec<TempIdRow> =
            sqlx::query_as(r#"SELECT "id" FROM recursive_folder_children_ids($1)"#)
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
        let (file_count, link_count, folder_count): (i64, i64, i64) =
            sqlx::query_as(r#"SELECT * FROM count_folder_children($1) AS "counts""#)
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
        sqlx::query_as(r#"SELECT "id", "name" FROM resolve_folder_path($1)"#)
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

    pub async fn set_pinned(mut self, db: impl DbExecutor<'_>, pinned: bool) -> DbResult<Folder> {
        sqlx::query(r#"UPDATE "docbox_folders" SET "pinned" = $1 WHERE "id" = $2"#)
            .bind(pinned)
            .bind(self.id)
            .execute(db)
            .await?;

        self.pinned = pinned;

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
        folders_scope_with_id: Vec<DocboxInputPair<'_>>,
    ) -> DbResult<Vec<WithFullPath<FolderWithExtra>>> {
        if folders_scope_with_id.is_empty() {
            return Ok(Vec::new());
        }

        sqlx::query_as(r#"SELECT * FROM resolve_folders_with_extra_mixed_scopes($1)"#)
            .bind(folders_scope_with_id)
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

        sqlx::query_as(r#"SELECT * FROM resolve_folders_with_extra($1, $2)"#)
            .bind(scope)
            .bind(folder_ids)
            .fetch_all(db)
            .await
    }

    pub async fn find_by_id_with_extra(
        db: impl DbExecutor<'_>,
        scope: &DocumentBoxScopeRaw,
        id: FolderId,
    ) -> DbResult<Option<FolderWithExtra>> {
        sqlx::query_as(r#"SELECT * FROM resolve_folder_by_id_with_extra($1, $2)"#)
            .bind(scope)
            .bind(id)
            .fetch_optional(db)
            .await
    }

    pub async fn find_by_parent_with_extra(
        db: impl DbExecutor<'_>,
        parent_id: FolderId,
    ) -> DbResult<Vec<FolderWithExtra>> {
        sqlx::query_as(r#"SELECT * FROM resolve_folder_by_parent_with_extra($1)"#)
            .bind(parent_id)
            .fetch_all(db)
            .await
    }

    pub async fn find_root_with_extra(
        db: impl DbExecutor<'_>,
        document_box: &DocumentBoxScopeRaw,
    ) -> DbResult<Option<FolderWithExtra>> {
        sqlx::query_as(r#"SELECT * FROM resolve_root_folder_with_extra($1)"#)
            .bind(document_box)
            .fetch_optional(db)
            .await
    }

    /// Get the total number of folders in the tenant
    pub async fn total_count(db: impl DbExecutor<'_>) -> DbResult<i64> {
        let count_result: CountResult =
            sqlx::query_as(r#"SELECT COUNT(*) AS "count" FROM "docbox_folders""#)
                .fetch_one(db)
                .await?;

        Ok(count_result.count)
    }
}
