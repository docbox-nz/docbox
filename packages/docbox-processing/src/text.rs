use bytes::Bytes;
use docbox_database::models::generated_file::GeneratedFileType;
use docbox_search::models::DocumentPage;
use mime::Mime;

use super::{ProcessingIndexMetadata, ProcessingOutput, QueuedUpload};

const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

/// Checks if the provided mime should be processed as text
///
/// Matches any `text/*` type
pub fn is_text_file(mime: &Mime) -> bool {
    if mime.type_() == mime::TEXT {
        return true;
    }

    false
}

/// Checks if the provided mime should be processed as text
/// but from a application file (json/xml)
///
/// Matches any JSON (`application/json` and `+json` subtypes),
/// and XML (`application/xml` and `+xml` subtypes).
pub fn is_application_file(mime: &Mime) -> bool {
    if mime.type_() != mime::APPLICATION {
        return false;
    }

    let subtype = mime.subtype().as_str();
    if subtype == "json" || subtype == "xml" {
        return true;
    }

    let suffix = mime.suffix();
    suffix.is_some_and(|suffix| matches!(suffix.as_str(), "json" | "xml"))
}

/// Processes a text, JSON, or XML file by decoding the source bytes
/// and emitting searchable [GeneratedFileType::TextContent]
pub fn process_text(file_bytes: &[u8]) -> ProcessingOutput {
    let raw = file_bytes.strip_prefix(UTF8_BOM).unwrap_or(file_bytes);
    let content = String::from_utf8_lossy(raw).into_owned();

    let pages = vec![DocumentPage {
        content: content.clone(),
        page: 0,
    }];

    ProcessingOutput {
        encrypted: false,
        additional_files: Vec::new(),
        index_metadata: Some(ProcessingIndexMetadata { pages: Some(pages) }),
        upload_queue: vec![QueuedUpload::new(
            mime::TEXT_PLAIN,
            GeneratedFileType::TextContent,
            Bytes::from(content),
        )],
    }
}
