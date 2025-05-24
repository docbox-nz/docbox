use axum::Json;
use docbox_core::utils::validation::ALLOWED_MIME_TYPES;

use crate::{models::document_box::DocumentBoxOptions, MAX_FILE_SIZE};

pub const UTILS_TAG: &str = "utils";

/// Get options
///
/// Requests options and settings from docbox
#[utoipa::path(
    get,
    tag = UTILS_TAG,
    path = "/options",
    responses(
        (status = 200, description = "Got settings successfully", body = DocumentBoxOptions),
    )
)]
pub async fn get_options() -> Json<DocumentBoxOptions> {
    Json(DocumentBoxOptions {
        allowed_mime_types: ALLOWED_MIME_TYPES,
        max_file_size: MAX_FILE_SIZE,
    })
}
