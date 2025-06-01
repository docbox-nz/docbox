use docbox_database::models::document_box::DocumentBoxScopeRaw;
use mime::Mime;
use uuid::Uuid;

use crate::utils::file::{get_file_name_ext, get_mime_ext, make_s3_safe};

pub mod delete_file;
pub mod generated;
pub mod index_file;
pub mod update_file;
pub mod upload_file;
pub mod upload_file_presigned;

pub fn create_file_key(document_box: &DocumentBoxScopeRaw, name: &str, mime: &Mime) -> String {
    // Try get file extension from name
    let file_ext = get_file_name_ext(name)
        // Fallback to extension from mime type
        .or_else(|| get_mime_ext(mime).map(|value| value.to_string()))
        // Fallback to default .bin extension
        .unwrap_or_else(|| "bin".to_string());

    // Get the file name with the file extension stripped
    let file_name = name.strip_suffix(&file_ext).unwrap_or(name);

    // Strip unwanted characters from the file name
    let clean_file_name = make_s3_safe(file_name);

    // Unique portion of the file key
    let file_key_unique = Uuid::new_v4().to_string();

    // Key is composed of the {Unique ID}_{File Name}.{File Ext}
    let file_key = format!("{file_key_unique}_{clean_file_name}.{file_ext}");

    // Prefix file key with the scope directory
    format!("{}/{}", document_box, file_key)
}
