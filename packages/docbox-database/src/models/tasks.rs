use super::document_box::DocumentBoxScopeRaw;
use crate::{DbExecutor, DbResult, models::document_box::DocumentBoxScopeRawRef};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Database, Decode, error::BoxDynError, prelude::FromRow};
use utoipa::ToSchema;
use uuid::Uuid;

pub type TaskId = Uuid;

/// Represents a stored asynchronous task progress
#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct Task {
    /// Unique ID of the task
    pub id: Uuid,

    /// ID of the document box the task belongs to
    pub document_box: DocumentBoxScopeRaw,

    /// Status of the task
    pub status: TaskStatus,

    /// Output data from the task completion
    pub output_data: Option<serde_json::Value>,

    /// When the task was created
    pub created_at: DateTime<Utc>,

    // When execution of the task completed
    pub completed_at: Option<DateTime<Utc>>,
}

impl Eq for Task {}

impl PartialEq for Task {
    fn eq(&self, other: &Self) -> bool {
        let complete_eq = match (&self.completed_at, &other.completed_at) {
            (Some(a), Some(b)) => a.timestamp_millis().eq(&b.timestamp_millis()),
            (None, None) => true,
            _ => false,
        };

        self.id.eq(&other.id)
            && self.document_box.eq(&other.document_box)
            && self.status.eq(&other.status)
            && self.output_data.eq(&self.output_data)
            // Reduce precision when checking creation timestamp
            // (Database does not store the full precision)
            && self
                .created_at
                .timestamp_millis()
                .eq(&other.created_at.timestamp_millis())
            // Reduce precision when checking creation timestamp
            // (Database does not store the full precision)
            && complete_eq
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    strum::EnumString,
    strum::Display,
    Deserialize,
    Serialize,
    ToSchema,
    PartialEq,
    Eq,
)]
pub enum TaskStatus {
    Pending,
    Completed,
    Failed,
}

impl<DB: Database> sqlx::Type<DB> for TaskStatus
where
    String: sqlx::Type<DB>,
{
    fn type_info() -> DB::TypeInfo {
        String::type_info()
    }
}

impl<'r, DB: Database> Decode<'r, DB> for TaskStatus
where
    String: Decode<'r, DB>,
{
    fn decode(value: <DB as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let value = <String as Decode<DB>>::decode(value)?;
        Ok(value.parse()?)
    }
}

impl Task {
    /// Stores / updates the stored user data, returns back the user ID
    pub async fn create(
        db: impl DbExecutor<'_>,
        document_box: DocumentBoxScopeRaw,
    ) -> DbResult<Task> {
        let task_id = Uuid::new_v4();
        let status = TaskStatus::Pending;
        let created_at = Utc::now();

        sqlx::query(
            r#"
            INSERT INTO "docbox_tasks" ("id", "document_box", "status", "created_at")
            VALUES ($1, $2, $3, $4)
        "#,
        )
        .bind(task_id)
        .bind(document_box.as_str())
        .bind(status.to_string())
        .bind(created_at)
        .execute(db)
        .await?;

        Ok(Task {
            id: task_id,
            document_box,
            status,
            output_data: None,
            created_at,
            completed_at: None,
        })
    }

    pub async fn find(
        db: impl DbExecutor<'_>,
        id: TaskId,
        document_box: DocumentBoxScopeRawRef<'_>,
    ) -> DbResult<Option<Task>> {
        sqlx::query_as(r#"SELECT * FROM "docbox_tasks" WHERE "id" = $1 AND "document_box" = $2"#)
            .bind(id)
            .bind(document_box)
            .fetch_optional(db)
            .await
    }

    /// Mark the task as completed and set its output data
    pub async fn complete_task(
        &mut self,
        db: impl DbExecutor<'_>,
        status: TaskStatus,
        output_data: Option<serde_json::Value>,
    ) -> DbResult<()> {
        let completed_at = Utc::now();

        sqlx::query(
            r#"UPDATE "docbox_tasks" SET
            "status" = $1,
            "output_data" = $2,
            "completed_at" = $3
            WHERE "id" = $4"#,
        )
        .bind(status.to_string())
        .bind(output_data.clone())
        .bind(completed_at)
        .bind(self.id)
        .execute(db)
        .await?;

        self.status = status;
        self.output_data = output_data.clone();
        self.completed_at = Some(completed_at);

        Ok(())
    }
}
