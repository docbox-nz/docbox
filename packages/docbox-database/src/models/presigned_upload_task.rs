//! # Presigned Upload Task
//!
//! Background uploading task, handles storing data about pending
//! pre-signed S3 file uploads. Used to track completion and uploads
//! that were cancelled or failed

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgQueryResult, prelude::FromRow, types::Json};
use uuid::Uuid;

use super::{document_box::DocumentBoxScopeRaw, file::FileId, folder::FolderId, user::UserId};
use crate::{DbErr, DbExecutor, DbResult};

pub type PresignedUploadTaskId = Uuid;

/// Task storing the details for a presigned upload task
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct PresignedUploadTask {
    /// ID of the upload task
    pub id: PresignedUploadTaskId,
    /// File created from the outcome of this task
    #[sqlx(json)]
    pub status: PresignedTaskStatus,

    /// Name of the file being uploaded
    pub name: String,
    /// Mime type of the file being uploaded
    pub mime: String,
    /// Expected size in bytes of the file
    pub size: i32,

    /// ID of the document box the folder belongs to
    pub document_box: DocumentBoxScopeRaw,
    /// Target folder to store the file in
    pub folder_id: FolderId,
    /// S3 key where the file should be stored
    pub file_key: String,

    /// Creation timestamp of the upload task
    pub created_at: DateTime<Utc>,
    /// Timestamp of when the presigned URL will expire
    /// (Used as the date for background cleanup, to ensure that all files)
    pub expires_at: DateTime<Utc>,
    /// User who created the file
    pub created_by: Option<UserId>,

    /// Optional file to make the parent of this file
    pub parent_id: Option<FileId>,

    /// Config that can be used when processing for additional
    /// configuration to how the file is processed
    pub processing_config: Option<Json<serde_json::Value>>,
}

impl Eq for PresignedUploadTask {}

impl PartialEq for PresignedUploadTask {
    fn eq(&self, other: &Self) -> bool {
        self.id.eq(&other.id)
            && self.status.eq(&other.status)
            && self.name.eq(&other.name)
            && self.mime.eq(&other.mime)
            && self.size.eq(&other.size)
            && self.document_box.eq(&other.document_box)
            && self.folder_id.eq(&other.folder_id)
            && self.file_key.eq(&other.file_key)
            // Reduce precision when checking creation timestamp
            // (Database does not store the full precision)
            && self
                .created_at
                .timestamp_millis()
                .eq(&other.created_at.timestamp_millis())
            // Reduce precision when checking creation timestamp
            // (Database does not store the full precision)
            && self
                .expires_at
                .timestamp_millis()
                .eq(&other.expires_at.timestamp_millis())
            && self.created_by.eq(&self.created_by)
            && self.parent_id.eq(&other.parent_id)
            && self.processing_config.eq(&other.processing_config)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "status")]
pub enum PresignedTaskStatus {
    Pending,
    Completed { file_id: FileId },
    Failed { error: String },
}

/// Required data to create a presigned upload task
#[derive(Default)]
pub struct CreatePresignedUploadTask {
    pub name: String,
    pub mime: String,
    pub document_box: DocumentBoxScopeRaw,
    pub folder_id: FolderId,
    pub size: i32,
    pub file_key: String,
    pub created_by: Option<UserId>,
    pub expires_at: DateTime<Utc>,
    pub parent_id: Option<FileId>,
    pub processing_config: Option<serde_json::Value>,
}

impl PresignedUploadTask {
    /// Create a new presigned upload task
    pub async fn create(
        db: impl DbExecutor<'_>,
        create: CreatePresignedUploadTask,
    ) -> DbResult<PresignedUploadTask> {
        let id = Uuid::new_v4();
        let created_at = Utc::now();

        let task = PresignedUploadTask {
            id,
            status: PresignedTaskStatus::Pending,
            //
            name: create.name,
            mime: create.mime,
            size: create.size,
            //
            document_box: create.document_box,
            folder_id: create.folder_id,
            file_key: create.file_key,
            //
            created_at,
            expires_at: create.expires_at,
            created_by: create.created_by,

            parent_id: create.parent_id,
            processing_config: create.processing_config.map(Json),
        };

        let status_json =
            serde_json::to_value(&task.status).map_err(|err| DbErr::Encode(Box::new(err)))?;
        let processing_config_json = task.processing_config.clone();

        sqlx::query(
            r#"
            INSERT INTO "docbox_presigned_upload_tasks" (
                "id",
                "status",
                "name",
                "mime",
                "size",
                "document_box",
                "folder_id",
                "file_key",
                "created_at",
                "expires_at",
                "created_by",
                "parent_id",
                "processing_config"
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            "#,
        )
        .bind(task.id)
        .bind(status_json)
        .bind(task.name.as_str())
        .bind(task.mime.clone())
        .bind(task.size)
        .bind(task.document_box.as_str())
        .bind(task.folder_id)
        .bind(task.file_key.as_str())
        .bind(task.created_at)
        .bind(task.expires_at)
        .bind(task.created_by.clone())
        .bind(task.parent_id)
        .bind(processing_config_json)
        .execute(db)
        .await?;

        Ok(task)
    }

    pub async fn set_status(
        &mut self,
        db: impl DbExecutor<'_>,
        status: PresignedTaskStatus,
    ) -> DbResult<()> {
        let status_json =
            serde_json::to_value(&status).map_err(|err| DbErr::Encode(Box::new(err)))?;

        sqlx::query(r#"UPDATE "docbox_presigned_upload_tasks" SET "status" = $1 WHERE "id" = $2"#)
            .bind(status_json)
            .bind(self.id)
            .execute(db)
            .await?;

        self.status = status;
        Ok(())
    }

    /// Find a specific presigned upload task
    pub async fn find(
        db: impl DbExecutor<'_>,
        scope: &DocumentBoxScopeRaw,
        task_id: PresignedUploadTaskId,
    ) -> DbResult<Option<PresignedUploadTask>> {
        sqlx::query_as(
            r#"SELECT * FROM "docbox_presigned_upload_tasks"
            WHERE "id" = $1 AND "document_box" = $2"#,
        )
        .bind(task_id)
        .bind(scope)
        .fetch_optional(db)
        .await
    }

    /// Finds all presigned uploads that have expired based on the current date
    pub async fn find_expired(
        db: impl DbExecutor<'_>,
        current_date: DateTime<Utc>,
    ) -> DbResult<Vec<PresignedUploadTask>> {
        sqlx::query_as(r#"SELECT * FROM "docbox_presigned_upload_tasks" WHERE "expires_at" < $1"#)
            .bind(current_date)
            .fetch_all(db)
            .await
    }

    /// Find a specific presigned upload task
    pub async fn find_by_file_key(
        db: impl DbExecutor<'_>,
        file_key: &str,
    ) -> DbResult<Option<PresignedUploadTask>> {
        sqlx::query_as(r#"SELECT * FROM "docbox_presigned_upload_tasks" WHERE "file_key" = $1"#)
            .bind(file_key)
            .fetch_optional(db)
            .await
    }

    /// Delete a specific presigned upload task
    pub async fn delete(
        db: impl DbExecutor<'_>,
        task_id: PresignedUploadTaskId,
    ) -> DbResult<PgQueryResult> {
        sqlx::query(r#"DELETE FROM "docbox_presigned_upload_tasks" WHERE "id" = $1"#)
            .bind(task_id)
            .execute(db)
            .await
    }
}
