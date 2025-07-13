use mime::Mime;
use uuid::Uuid;

use crate::utils::file::{get_file_name_ext, get_mime_ext, make_s3_safe};

pub mod delete_file;
pub mod generated;
pub mod index_file;
pub mod purge_expired_presigned_tasks;
pub mod reprocess_octet_stream_files;
pub mod update_file;
pub mod upload_file;
pub mod upload_file_presigned;

pub fn create_file_key(document_box: &str, name: &str, mime: &Mime, file_key: Uuid) -> String {
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

    // Key is composed of the {Unique ID}_{File Name}.{File Ext}
    let file_key = format!("{file_key}_{clean_file_name}.{file_ext}");

    // Prefix file key with the scope directory
    format!("{}/{}", document_box, file_key)
}

pub fn create_generated_file_key(base_file_key: &str, mime: &Mime) -> String {
    // Mapped file extensions for the generated type
    let file_ext = get_mime_ext(mime).unwrap_or("bin");

    // Generate a unique file key
    let file_key = Uuid::new_v4().to_string();

    // Prefix the file key with the document box scope and a "generated" suffix
    format!("{}_{}.generated.{}", base_file_key, file_key, file_ext)
}

#[cfg(test)]
mod test {
    use crate::files::create_file_key;
    use mime::Mime;
    use uuid::Uuid;

    #[test]
    fn test_create_file_key_ext_from_mime() {
        let scope = "scope";
        let mime: Mime = "image/png".parse().unwrap();
        let file_key = Uuid::new_v4();
        let key = create_file_key(scope, "photo", &mime, file_key);

        assert_eq!(key, format!("scope/{file_key}_photo.png"));
    }

    #[test]
    fn test_create_file_key_fallback_bin() {
        let scope = "scope";
        let mime: Mime = "unknown/unknown".parse().unwrap();
        let file_key = Uuid::new_v4();
        let key = create_file_key(scope, "file", &mime, file_key);

        assert_eq!(key, format!("scope/{file_key}_file.bin"));
    }

    #[test]
    fn test_create_file_key_strips_special_chars() {
        let scope = "scope";
        let mime: Mime = "text/plain".parse().unwrap();
        let file_key = Uuid::new_v4();
        let key = create_file_key(scope, "some file$name.txt", &mime, file_key);

        assert_eq!(key, format!("scope/{file_key}_some_filename.txt"));
    }
}
