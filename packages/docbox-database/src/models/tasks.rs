use std::future::Future;

use crate::{DbExecutor, DbPool, DbResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{error::BoxDynError, prelude::FromRow, Database, Decode};
use uuid::Uuid;

use super::document_box::DocumentBoxScope;

pub type TaskId = Uuid;

/// Represents a stored asynchronous task progress
#[derive(Debug, FromRow, Serialize)]
pub struct Task {
    /// Unique ID of the task
    pub id: TaskId,

    /// ID of the document box the task belongs to
    pub document_box: DocumentBoxScope,

    /// Status of the task
    pub status: TaskStatus,

    /// Output data from the task completion
    pub output_data: Option<serde_json::Value>,

    /// When the task was created
    pub created_at: DateTime<Utc>,

    // When execution of the task completed
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, strum::EnumString, strum::Display, Deserialize, Serialize)]
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

pub async fn background_task<Fut>(
    db: DbPool,
    scope: DocumentBoxScope,
    future: Fut,
) -> anyhow::Result<(TaskId, DateTime<Utc>)>
where
    Fut: Future<Output = (TaskStatus, serde_json::Value)> + Send + 'static,
{
    // Create task for progression
    let task = match Task::create(&db, scope).await {
        Ok(value) => value,
        Err(cause) => {
            tracing::error!(?cause, "failed to create upload task");
            anyhow::bail!("failed to create upload task")
        }
    };

    let task_id = task.id;
    let created_at = task.created_at;

    // Swap background task
    tokio::spawn(async move {
        let (status, output) = future.await;

        // Update task completion
        if let Err(cause) = task.complete_task(&db, status, Some(output)).await {
            tracing::error!(?cause, "failed to mark task as complete");
        }
    });

    Ok((task_id, created_at))
}

impl Task {
    /// Stores / updates the stored user data, returns back the user ID
    pub async fn create(db: impl DbExecutor<'_>, document_box: DocumentBoxScope) -> DbResult<Task> {
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
        document_box: DocumentBoxScope,
    ) -> DbResult<Option<Task>> {
        sqlx::query_as(r#"SELECT * FROM "docbox_tasks" WHERE "id" = $1 AND "document_box" = $2"#)
            .bind(id)
            .bind(document_box.as_str())
            .fetch_optional(db)
            .await
    }

    /// Mark the task as completed and set its output data
    pub async fn complete_task(
        mut self,
        db: impl DbExecutor<'_>,
        status: TaskStatus,
        output_data: Option<serde_json::Value>,
    ) -> DbResult<Task> {
        let completed_at = Utc::now();

        sqlx::query(
            r#"UPDATE "docbox_tasks" SET 
            "status" = $1, 
            "output_data" = $2, 
            "completed_at" = $3"#,
        )
        .bind(status.to_string())
        .bind(output_data.clone())
        .bind(completed_at)
        .execute(db)
        .await?;

        self.status = status;
        self.output_data = output_data.clone();
        self.completed_at = Some(completed_at);

        Ok(self)
    }
}
