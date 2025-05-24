use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;

use super::{document_box::DocumentBoxScope, file::FileId};
use crate::{DbExecutor, DbResult};

pub type GeneratedFileId = Uuid;

#[derive(
    Debug, Clone, Copy, strum::EnumString, strum::Display, Deserialize, Serialize, ToSchema,
)]
pub enum GeneratedFileType {
    /// Conversion to PDF file
    Pdf,
    /// Full sized cover page render
    CoverPage,
    /// Small file sized thumbnail image
    SmallThumbnail,
    /// Larger thumbnail image, for a small preview tooltip
    LargeThumbnail,
    /// Text content extracted from the file
    TextContent,
    /// HTML content extracted from things like emails
    HtmlContent,
    /// JSON encoded metadata for the file
    /// (Used by emails to store the email metadata in an accessible ways)
    Metadata,
}

impl TryFrom<String> for GeneratedFileType {
    type Error = strum::ParseError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        GeneratedFileType::from_str(&value)
    }
}

/// File generated as an artifact of an uploaded file
#[derive(Debug, FromRow, Serialize, ToSchema)]
pub struct GeneratedFile {
    /// Unique identifier for the file
    #[schema(value_type = Uuid)]
    pub id: GeneratedFileId,
    /// File this generated file belongs  to
    #[schema(value_type = Uuid)]
    pub file_id: FileId,
    /// Mime type of the generated file content
    pub mime: String,
    /// Type of the generated file
    #[sqlx(rename = "type")]
    #[serde(rename = "type")]
    #[sqlx(try_from = "String")]
    pub ty: GeneratedFileType,
    /// Hash of the file this was generated from
    pub hash: String,
    /// S3 key pointing to the file
    #[serde(skip)]
    pub file_key: String,
    /// When the file was created
    pub created_at: DateTime<Utc>,
}

pub struct CreateGeneratedFile {
    pub file_id: FileId,
    pub mime: String,
    pub ty: GeneratedFileType,
    pub hash: String,
    pub file_key: String,
}

impl GeneratedFile {
    pub async fn create(
        db: impl DbExecutor<'_>,
        CreateGeneratedFile {
            file_id,
            ty,
            hash,
            file_key,
            mime,
        }: CreateGeneratedFile,
    ) -> DbResult<GeneratedFile> {
        let id = Uuid::new_v4();
        let created_at = Utc::now();

        sqlx::query(
            r#"
            INSERT INTO "docbox_generated_files" 
            ("id", "file_id", "mime", "type", "hash", "file_key", "created_at")
            VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
        )
        .bind(id)
        .bind(file_id)
        .bind(mime.as_str())
        .bind(ty.to_string())
        .bind(hash.as_str())
        .bind(file_key.as_str())
        .bind(created_at)
        .execute(db)
        .await?;

        Ok(GeneratedFile {
            id,
            file_id,
            mime,
            ty,
            hash,
            file_key,
            created_at,
        })
    }

    /// Deletes the generated file
    pub async fn delete(self, db: impl DbExecutor<'_>) -> DbResult<()> {
        sqlx::query(r#"DELETE FROM "docbox_generated_files" WHERE "id" = $1"#)
            .bind(self.id)
            .execute(db)
            .await?;

        Ok(())
    }

    pub async fn find_all(
        db: impl DbExecutor<'_>,
        file_id: FileId,
    ) -> DbResult<Vec<GeneratedFile>> {
        sqlx::query_as(r#"SELECT * FROM "docbox_generated_files" WHERE "file_id" = $1"#)
            .bind(file_id)
            .fetch_all(db)
            .await
    }

    /// Finds a specific file using its full path scope -> folder -> file
    pub async fn find(
        db: impl DbExecutor<'_>,
        scope: &DocumentBoxScope,
        file_id: FileId,
        ty: GeneratedFileType,
    ) -> DbResult<Option<GeneratedFile>> {
        sqlx::query_as(
            r#"
            SELECT "gen".*
            FROM "docbox_generated_files" "gen"
            -- Join on the file itself
            INNER JOIN "docbox_files" "file" ON "gen".file_id = "file"."id"
            -- Join to the file parent folder
            INNER JOIN "docbox_folders" "folder" ON "file"."folder_id" = "folder"."id"
            -- Only find the matching type for the specified file
            WHERE "file"."id" = $1 AND "folder"."document_box" = $2 AND "gen"."type" = $3
        "#,
        )
        .bind(file_id)
        .bind(scope)
        .bind(ty.to_string())
        .fetch_optional(db)
        .await
    }
}
