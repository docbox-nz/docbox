use axum::{Extension, Json, http::StatusCode};
use docbox_core::notifications::{
    MpscNotificationQueueSender, NotificationQueueMessage, parse_bucket_message,
};

use crate::{
    MAX_FILE_SIZE,
    error::{DynHttpError, HttpCommonError},
    models::document_box::DocumentBoxOptions,
};

pub const UTILS_TAG: &str = "Utils";

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
pub async fn get_options() -> Json<DocumentBoxOptions> {
    Json(DocumentBoxOptions {
        max_file_size: MAX_FILE_SIZE,
    })
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
