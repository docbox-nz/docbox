use bytes::Bytes;
use docbox_database::models::generated_file::GeneratedFileType;
use docbox_search::models::DocumentPage;
use mime::Mime;

use super::{ProcessingIndexMetadata, ProcessingOutput, QueuedUpload};

const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

/// Checks if the provided mime should be processed as source text
///
/// Matches any `text/*` type, JSON (`application/json` and `+json` subtypes),
/// and XML (`application/xml` and `+xml` subtypes).
pub fn is_text_mime(mime: &Mime) -> bool {
    if mime.type_() == mime::TEXT {
        return true;
    }

    if mime.type_() == mime::APPLICATION {
        let subtype = mime.subtype().as_str();
        return subtype == "json"
            || subtype == "xml"
            || subtype.ends_with("+json")
            || subtype.ends_with("+xml");
    }

    false
}

/// Processes a text, JSON, or XML file by decoding the source bytes
/// and emitting searchable [GeneratedFileType::TextContent]
pub fn process_text(file_bytes: &[u8]) -> ProcessingOutput {
    let raw = if file_bytes.starts_with(UTF8_BOM) {
        &file_bytes[UTF8_BOM.len()..]
    } else {
        file_bytes
    };

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
            Bytes::from(content.into_bytes()),
        )],
    }
}
