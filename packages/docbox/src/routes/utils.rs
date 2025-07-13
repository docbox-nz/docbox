use axum::{Extension, Json, http::StatusCode};
use docbox_core::notifications::{
    MpscNotificationQueueSender, NotificationQueueMessage, parse_bucket_message,
};

use crate::{
    error::{DynHttpError, HttpCommonError},
    extensions::max_file_size::MaxFileSizeBytes,
    models::document_box::DocumentBoxOptions,
};

pub const UTILS_TAG: &str = "Utils";

/// Health check
///
/// Check that the server is running using this endpoint
#[utoipa::path(
    get,
    operation_id = "health",
    tag = UTILS_TAG,
    path = "/health",
    responses(
        (status = 200, description = "Health check success")
    )
)]
pub async fn health() -> StatusCode {
    StatusCode::OK
}

/// Get options
///
/// Requests options and settings from docbox
#[utoipa::path(
    get,
    operation_id = "options",
    tag = UTILS_TAG,
    path = "/options",
    responses(
        (status = 200, description = "Got settings successfully", body = DocumentBoxOptions)
    )
)]
pub async fn get_options(
    Extension(MaxFileSizeBytes(max_file_size)): Extension<MaxFileSizeBytes>,
) -> Json<DocumentBoxOptions> {
    Json(DocumentBoxOptions { max_file_size })
}

/// POST /webhook/s3
///
/// Internal endpoint for handling requests from a webhook
pub async fn webhook_s3(
    Extension(tx): Extension<MpscNotificationQueueSender>,
    Json(req): Json<serde_json::Value>,
) -> Result<StatusCode, DynHttpError> {
    tracing::debug!(?req, "got webhook s3 event");

    let (bucket_name, object_key) = parse_bucket_message(&req).ok_or_else(|| {
        tracing::warn!("failed to handle webhook s3 event");
        HttpCommonError::ServerError
    })?;

    tx.send(NotificationQueueMessage::FileCreated {
        bucket_name,
        object_key,
    })
    .await;

    Ok(StatusCode::OK)
}
