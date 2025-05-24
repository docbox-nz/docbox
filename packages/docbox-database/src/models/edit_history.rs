//! Database structure that tracks changes to files

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::str::FromStr;
use utoipa::ToSchema;
use uuid::Uuid;

use super::{file::FileId, folder::FolderId, user::UserId};
use crate::models::link::LinkId;
use crate::{DbErr, DbExecutor, DbResult};

pub type EditHistoryId = Uuid;

#[derive(
    Debug, Clone, Copy, strum::EnumString, strum::Display, Deserialize, Serialize, ToSchema,
)]
pub enum EditHistoryType {
    /// File was moved to a different folder
    MoveToFolder,
    /// File was renamed
    Rename,
    /// Link value was changed
    LinkValue,
}

impl TryFrom<String> for EditHistoryType {
    type Error = strum::ParseError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        EditHistoryType::from_str(&value)
    }
}

/// Metadata associated with an edit history
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
#[serde(tag = "type")]
pub enum EditHistoryMetadata {
    MoveToFolder {
        /// Folder moved from
        #[schema(value_type = Option<Uuid>)]
        original_id: Option<FolderId>,
        /// Folder moved to
        #[schema(value_type = Uuid)]
        target_id: FolderId,
    },

    Rename {
        /// Previous name
        original_name: String,
        /// New name
        new_name: String,
    },

    LinkValue {
        /// Previous URL
        previous_value: String,
        /// New URL
        new_value: String,
    },
}

#[derive(Debug, Serialize, FromRow, ToSchema)]
pub struct EditHistory {
    /// Unique identifier for this history entry
    #[schema(value_type = Uuid)]
    pub id: EditHistoryId,

    /// ID of the file that was edited (If a file was edited)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Uuid>)]
    pub file_id: Option<FileId>,
    /// ID of the file that was edited (If a link was edited)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Uuid>)]
    pub link_id: Option<LinkId>,
    /// ID of the file that was edited (If a folder was edited)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Uuid>)]
    pub folder_id: Option<FolderId>,

    /// User that made the edit
    #[sqlx(flatten)]
    pub user: EditHistoryUser,

    /// The type of change that was made
    #[serde(rename = "type")]
    #[sqlx(rename = "type")]
    #[sqlx(try_from = "String")]
    pub ty: EditHistoryType,

    /// Metadata associated with the change
    #[schema(value_type = EditHistoryMetadata)]
    pub metadata: sqlx::types::Json<EditHistoryMetadata>,

    /// When this change was made
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, FromRow, ToSchema)]
pub struct EditHistoryUser {
    /// Unique ID of the user
    #[sqlx(rename = "user_id")]
    pub id: Option<String>,
    /// Last saved name for the user
    #[sqlx(rename = "user_name")]
    pub name: Option<String>,
    /// Last saved image ID for the user
    #[sqlx(rename = "user_image_id")]
    pub image_id: Option<String>,
}

pub struct CreateEditHistory {
    pub ty: CreateEditHistoryType,
    pub user_id: Option<UserId>,
    pub metadata: EditHistoryMetadata,
}

pub enum CreateEditHistoryType {
    File(FileId),
    Folder(FolderId),
    Link(LinkId),
}

impl EditHistory {
    pub async fn create(
        db: impl DbExecutor<'_>,
        CreateEditHistory {
            ty,
            user_id,
            metadata,
        }: CreateEditHistory,
    ) -> DbResult<()> {
        let id = Uuid::new_v4();
        let created_at = Utc::now();

        let mut file_id: Option<FileId> = None;
        let mut folder_id: Option<FolderId> = None;
        let mut link_id: Option<LinkId> = None;

        match ty {
            CreateEditHistoryType::File(id) => file_id = Some(id),
            CreateEditHistoryType::Folder(id) => folder_id = Some(id),
            CreateEditHistoryType::Link(id) => link_id = Some(id),
        }

        let ty = match &metadata {
            EditHistoryMetadata::MoveToFolder { .. } => EditHistoryType::MoveToFolder,
            EditHistoryMetadata::Rename { .. } => EditHistoryType::Rename,
            EditHistoryMetadata::LinkValue { .. } => EditHistoryType::LinkValue,
        };

        let metadata = serde_json::to_value(&metadata).map_err(|err| DbErr::Encode(err.into()))?;

        sqlx::query(
            r#"
            INSERT INTO "docbox_edit_history" (
                "id", "file_id", "link_id",
                "folder_id", "user_id", "type", 
                "metadata", "created_at"
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
        )
        .bind(id)
        .bind(file_id)
        .bind(link_id)
        .bind(folder_id)
        .bind(user_id)
        .bind(ty.to_string())
        .bind(metadata)
        .bind(created_at)
        .execute(db)
        .await?;

        Ok(())
    }

    pub async fn all_by_file(
        db: impl DbExecutor<'_>,
        file_id: FileId,
    ) -> DbResult<Vec<EditHistory>> {
        sqlx::query_as(
            r#"
            SELECT 
                "history".*,
                "user"."id" AS "user_id",
                "user"."name" AS "user_name",
                "user"."image_id" AS "user_image_id"
            FROM "docbox_edit_history" "history"
            LEFT JOIN
                "docbox_users" "user" ON "history"."user_id" = "user"."id" 
            WHERE "history"."file_id" = $1
            ORDER BY "history"."created_at" DESC
        "#,
        )
        .bind(file_id)
        .fetch_all(db)
        .await
    }

    pub async fn all_by_folder(
        db: impl DbExecutor<'_>,
        folder_id: FolderId,
    ) -> DbResult<Vec<EditHistory>> {
        sqlx::query_as(
            r#"
            SELECT 
                "history".*,
                "user"."id" AS "user_id",
                "user"."name" AS "user_name",
                "user"."image_id" AS "user_image_id"
            FROM "docbox_edit_history" "history"
            LEFT JOIN
                "docbox_users" "user" ON "history"."user_id" = "user"."id" 
            WHERE "history"."folder_id" = $1
            ORDER BY "history"."created_at" DESC
        "#,
        )
        .bind(folder_id)
        .fetch_all(db)
        .await
    }

    pub async fn all_by_link(
        db: impl DbExecutor<'_>,
        link_id: LinkId,
    ) -> DbResult<Vec<EditHistory>> {
        sqlx::query_as(
            r#"
            SELECT 
                "history".*,
                "user"."id" AS "user_id",
                "user"."name" AS "user_name",
                "user"."image_id" AS "user_image_id"
            FROM "docbox_edit_history" "history"
            LEFT JOIN
                "docbox_users" "user" ON "history"."user_id" = "user"."id" 
            WHERE "history"."link_id" = $1
            ORDER BY "history"."created_at" DESC
        "#,
        )
        .bind(link_id)
        .fetch_all(db)
        .await
    }
}
