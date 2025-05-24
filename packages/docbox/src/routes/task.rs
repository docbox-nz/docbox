//! File related endpoints

use crate::{error::HttpResult, middleware::tenant::TenantDb, models::file::HttpFileError};
use anyhow::Context;
use axum::{extract::Path, Json};
use docbox_database::models::{
    document_box::DocumentBoxScope,
    tasks::{Task, TaskId},
};

/// GET /box/:scope/task/:task_id
///
/// Gets a specific file details, metadata and associated
/// generated files
pub async fn get(
    TenantDb(db): TenantDb,
    Path((scope, task_id)): Path<(DocumentBoxScope, TaskId)>,
) -> HttpResult<Task> {
    let task = Task::find(&db, task_id, scope)
        .await
        .context("failed to query task")?
        .ok_or(HttpFileError::UnknownTask)?;

    Ok(Json(task))
}
