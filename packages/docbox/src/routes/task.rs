//! # Tasks
//!
//! Endpoints related to background tasks

use crate::{
    error::{HttpCommonError, HttpErrorResponse, HttpResult},
    middleware::tenant::{TenantDb, TenantParams},
    models::task::HttpTaskError,
};
use axum::{extract::Path, Json};
use docbox_database::models::{
    document_box::DocumentBoxScope,
    tasks::{Task, TaskId},
};

pub const TASK_TAG: &str = "Task";

/// Get task by ID
///
/// Get the details about a specific task, used to poll
/// the current progress of a task
#[utoipa::path(
    get,
    operation_id = "task_get",
    tag = TASK_TAG,
    path = "/box/{scope}/task/{task_id}",
    responses(
        (status = 200, description = "Task found successfully", body = Task),
        (status = 404, description = "Task not found", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(
        ("scope" = String, Path, description = "Scope the task is within"),
        ("task_id" = String, Path, description = "ID of the task to query"),
        TenantParams
    )
)]
pub async fn get(
    TenantDb(db): TenantDb,
    Path((scope, task_id)): Path<(DocumentBoxScope, TaskId)>,
) -> HttpResult<Task> {
    let task = Task::find(&db, task_id, &scope)
        .await
        // Failed to query the database
        .map_err(|cause| {
            tracing::error!(?scope, ?task_id, ?cause, "failed to query task");
            HttpCommonError::ServerError
        })?
        // Task not found
        .ok_or(HttpTaskError::UnknownTask)?;

    Ok(Json(task))
}
