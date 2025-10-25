use bytes::Bytes;
use docbox_database::models::generated_file::GeneratedFileType;
use docbox_processing::{
    ProcessingConfig, ProcessingLayerConfig, ProcessingOutput,
    email::{EmailEntity, EmailMetadataDocument},
    process_file,
};
use std::path::Path;

use crate::common::processing::{test_office_convert_server_container, test_processing_layer};

mod common;

/// Test processing a email file
#[tokio::test]
async fn test_process_email() {
    // Process the file
    let output = process_sample_file(None, "sample.eml")
        .await
        .expect("eml should produce processing output");

    assert!(
        !output.encrypted,
        "file was marked as encrypted but should not be"
    );

    assert_eq!(
        output.upload_queue.len(),
        3,
        "eml file should produce 1 metadata, 1 html content, and 1 text content"
    );

    // Ensure the files match the expectations
    let first = output.upload_queue.first().unwrap();
    assert_eq!(first.mime, mime::APPLICATION_JSON);
    assert!(matches!(first.ty, GeneratedFileType::Metadata));

    let second = output.upload_queue.get(1).unwrap();
    assert_eq!(second.mime, mime::TEXT_HTML);
    assert!(matches!(second.ty, GeneratedFileType::HtmlContent));

    let third = output.upload_queue.get(2).unwrap();
    assert_eq!(third.mime, mime::TEXT_PLAIN);
    assert!(matches!(third.ty, GeneratedFileType::TextContent));

    let metadata: EmailMetadataDocument =
        serde_json::from_slice(first.bytes.as_ref()).expect("metadata should be valid json");
    assert_eq!(metadata.date, Some("2025-06-08T14:10:47Z".to_string()));
    assert_eq!(metadata.subject, Some("Test email".to_string()));
    assert_eq!(metadata.message_id, Some("test-message-id".to_string()));
    assert_eq!(
        metadata.from,
        EmailEntity {
            name: Some("Example".to_string()),
            address: Some("example@example.com".to_string())
        }
    );
    assert_eq!(
        metadata.to,
        vec![EmailEntity {
            name: Some("Example (ExampleUser)".to_string()),
            address: Some("example@example.com".to_string())
        }]
    );

    assert!(metadata.cc.is_empty());
    assert!(metadata.bcc.is_empty());
    assert!(metadata.attachments.is_empty());

    // Ensure the html content matches expectation
    let html_content = String::from_utf8_lossy(second.bytes.as_ref());
    assert_eq!(
        html_content.as_ref().replace("\r\n", "\n"),
        "<div dir=\"ltr\">Test email body</div>\n"
    );

    // Ensure the text content matches expectation
    let text_content = String::from_utf8_lossy(third.bytes.as_ref());
    assert_eq!(
        text_content.as_ref().replace("\r\n", "\n"),
        "Test email body\n"
    );

    let index_metadata = output
        .index_metadata
        .expect("eml file should produce index metadata");

    // Ensure page content matches expectation
    let pages = index_metadata.pages.expect("eml file should produce pages");
    assert_eq!(pages.len(), 1);

    let first_page = pages.first().unwrap();
    assert_eq!(first_page.page, 0);
    assert_eq!(
        first_page.content.replace("\r\n", "\n"),
        "Test email body\n"
    );

    // Ensure no additional files are produced
    assert!(
        output.additional_files.is_empty(),
        "eml file without attachments should not produce additional files"
    );
}

/// Test processing a email file with a HTML content version
#[tokio::test]
async fn test_process_email_with_html() {
    // Process the file
    let output = process_sample_file(None, "sample_html_content.eml")
        .await
        .expect("eml should produce processing output");

    assert!(
        !output.encrypted,
        "file was marked as encrypted but should not be"
    );

    assert_eq!(
        output.upload_queue.len(),
        3,
        "eml file should produce 1 metadata, 1 html content, and 1 text content"
    );

    // Ensure the files match the expectations
    let first = output.upload_queue.first().unwrap();
    assert_eq!(first.mime, mime::APPLICATION_JSON);
    assert!(matches!(first.ty, GeneratedFileType::Metadata));

    let second = output.upload_queue.get(1).unwrap();
    assert_eq!(second.mime, mime::TEXT_HTML);
    assert!(matches!(second.ty, GeneratedFileType::HtmlContent));

    let third = output.upload_queue.get(2).unwrap();
    assert_eq!(third.mime, mime::TEXT_PLAIN);
    assert!(matches!(third.ty, GeneratedFileType::TextContent));

    let metadata: EmailMetadataDocument =
        serde_json::from_slice(first.bytes.as_ref()).expect("metadata should be valid json");
    assert_eq!(metadata.date, Some("2025-06-08T14:10:47Z".to_string()));
    assert_eq!(metadata.subject, Some("Test email".to_string()));
    assert_eq!(metadata.message_id, Some("test-message-id".to_string()));
    assert_eq!(
        metadata.from,
        EmailEntity {
            name: Some("Example".to_string()),
            address: Some("example@example.com".to_string())
        }
    );
    assert_eq!(
        metadata.to,
        vec![EmailEntity {
            name: Some("Example (ExampleUser)".to_string()),
            address: Some("example@example.com".to_string())
        }]
    );

    assert!(metadata.cc.is_empty());
    assert!(metadata.bcc.is_empty());
    assert!(metadata.attachments.is_empty());

    // Ensure the html content matches expectation
    let html_content = String::from_utf8_lossy(second.bytes.as_ref());
    assert_eq!(
        html_content.as_ref().replace("\r\n", "\n"),
        "<h1>Test title</h1>\n\n<div dir=\"ltr\"><b>Bold</b> Test email body block 1</div>\n<div dir=\"ltr\"><b>Bold</b> Test email body block 2</div>\n"
    );

    // Ensure the text content matches expectation
    let text_content = String::from_utf8_lossy(third.bytes.as_ref());
    assert_eq!(
        text_content.as_ref().replace("\r\n", "\n"),
        "Test title\n\nBold Test email body block 1\nBold Test email body block 2\n"
    );

    let index_metadata = output
        .index_metadata
        .expect("eml file should produce index metadata");

    // Ensure page content matches expectation
    let pages = index_metadata.pages.expect("eml file should produce pages");
    assert_eq!(pages.len(), 1);

    let first_page = pages.first().unwrap();
    assert_eq!(first_page.page, 0);
    assert_eq!(
        first_page.content.replace("\r\n", "\n"),
        "Test title\n\nBold Test email body block 1\nBold Test email body block 2\n"
    );

    // Ensure no additional files are produced
    assert!(
        output.additional_files.is_empty(),
        "eml file without attachments should not produce additional files"
    );
}

/// Test processing a email file with only a HTML content version
#[tokio::test]
async fn test_process_email_html_only() {
    // Process the file
    let output = process_sample_file(None, "sample_html_content_only.eml")
        .await
        .expect("eml should produce processing output");

    assert!(
        !output.encrypted,
        "file was marked as encrypted but should not be"
    );

    assert_eq!(
        output.upload_queue.len(),
        3,
        "eml file should produce 1 metadata, 1 html content, and 1 text content"
    );

    // Ensure the files match the expectations
    let first = output.upload_queue.first().unwrap();
    assert_eq!(first.mime, mime::APPLICATION_JSON);
    assert!(matches!(first.ty, GeneratedFileType::Metadata));

    let second = output.upload_queue.get(1).unwrap();
    assert_eq!(second.mime, mime::TEXT_HTML);
    assert!(matches!(second.ty, GeneratedFileType::HtmlContent));

    let third = output.upload_queue.get(2).unwrap();
    assert_eq!(third.mime, mime::TEXT_PLAIN);
    assert!(matches!(third.ty, GeneratedFileType::TextContent));

    let metadata: EmailMetadataDocument =
        serde_json::from_slice(first.bytes.as_ref()).expect("metadata should be valid json");
    assert_eq!(metadata.date, Some("2025-06-08T14:10:47Z".to_string()));
    assert_eq!(metadata.subject, Some("Test email".to_string()));
    assert_eq!(metadata.message_id, Some("test-message-id".to_string()));
    assert_eq!(
        metadata.from,
        EmailEntity {
            name: Some("Example".to_string()),
            address: Some("example@example.com".to_string())
        }
    );
    assert_eq!(
        metadata.to,
        vec![EmailEntity {
            name: Some("Example (ExampleUser)".to_string()),
            address: Some("example@example.com".to_string())
        }]
    );

    assert!(metadata.cc.is_empty());
    assert!(metadata.bcc.is_empty());
    assert!(metadata.attachments.is_empty());

    // Ensure the html content matches expectation
    let html_content = String::from_utf8_lossy(second.bytes.as_ref());
    assert_eq!(
        html_content.as_ref().replace("\r\n", "\n"),
        "<h1>Test title</h1>\n\n<div dir=\"ltr\"><b>Bold</b> Test email body block 1</div>\n<div dir=\"ltr\"><b>Bold</b> Test email body block 2</div>\n"
    );

    // Ensure the text content matches expectation
    let text_content = String::from_utf8_lossy(third.bytes.as_ref());
    assert_eq!(
        text_content.as_ref().replace("\r\n", "\n"),
        "Test title\n\n\nBold Test email body block 1\n\nBold Test email body block 2\n\n"
    );

    let index_metadata = output
        .index_metadata
        .expect("eml file should produce index metadata");

    // Ensure page content matches expectation
    let pages = index_metadata.pages.expect("eml file should produce pages");
    assert_eq!(pages.len(), 1);

    let first_page = pages.first().unwrap();
    assert_eq!(first_page.page, 0);
    assert_eq!(
        first_page.content.replace("\r\n", "\n"),
        "Test title\n\n\nBold Test email body block 1\n\nBold Test email body block 2\n\n"
    );

    // Ensure no additional files are produced
    assert!(
        output.additional_files.is_empty(),
        "eml file without attachments should not produce additional files"
    );
}

/// Test processing a email file with only a text content version
#[tokio::test]
async fn test_process_email_text_only() {
    // Process the file
    let output = process_sample_file(None, "sample_text_only.eml")
        .await
        .expect("eml should produce processing output");

    assert!(
        !output.encrypted,
        "file was marked as encrypted but should not be"
    );

    assert_eq!(
        output.upload_queue.len(),
        2,
        "eml file should produce 1 metadata, and 1 text content"
    );

    // Ensure the files match the expectations
    let first = output.upload_queue.first().unwrap();
    assert_eq!(first.mime, mime::APPLICATION_JSON);
    assert!(matches!(first.ty, GeneratedFileType::Metadata));

    let second = output.upload_queue.get(1).unwrap();
    assert_eq!(second.mime, mime::TEXT_PLAIN);
    assert!(matches!(second.ty, GeneratedFileType::TextContent));

    let metadata: EmailMetadataDocument =
        serde_json::from_slice(first.bytes.as_ref()).expect("metadata should be valid json");
    assert_eq!(metadata.date, Some("2025-06-08T14:10:47Z".to_string()));
    assert_eq!(metadata.subject, Some("Test email".to_string()));
    assert_eq!(metadata.message_id, Some("test-message-id".to_string()));
    assert_eq!(
        metadata.from,
        EmailEntity {
            name: Some("Example".to_string()),
            address: Some("example@example.com".to_string())
        }
    );
    assert_eq!(
        metadata.to,
        vec![EmailEntity {
            name: Some("Example (ExampleUser)".to_string()),
            address: Some("example@example.com".to_string())
        }]
    );

    assert!(metadata.cc.is_empty());
    assert!(metadata.bcc.is_empty());
    assert!(metadata.attachments.is_empty());

    // Ensure the text content matches expectation
    let text_content = String::from_utf8_lossy(second.bytes.as_ref());
    assert_eq!(
        text_content.as_ref().replace("\r\n", "\n"),
        "Test title, this is a plain text email content\n"
    );

    let index_metadata = output
        .index_metadata
        .expect("eml file should produce index metadata");

    // Ensure page content matches expectation
    let pages = index_metadata.pages.expect("eml file should produce pages");
    assert_eq!(pages.len(), 1);

    let first_page = pages.first().unwrap();
    assert_eq!(first_page.page, 0);
    assert_eq!(
        first_page.content.replace("\r\n", "\n"),
        "Test title, this is a plain text email content\n"
    );

    // Ensure no additional files are produced
    assert!(
        output.additional_files.is_empty(),
        "eml file without attachments should not produce additional files"
    );
}

/// Test processing a email file with an inline image attachment
/// (The attachment should be inlined using its content ID)
#[tokio::test]
async fn test_process_email_inline_attachment() {
    // Process the file
    let output = process_sample_file(None, "sample_inline_attachment.eml")
        .await
        .expect("eml should produce processing output");

    assert!(
        !output.encrypted,
        "file was marked as encrypted but should not be"
    );

    assert_eq!(
        output.upload_queue.len(),
        3,
        "eml file should produce 1 metadata, 1 html content, and 1 text content"
    );

    // Ensure the files match the expectations
    let first = output.upload_queue.first().unwrap();
    assert_eq!(first.mime, mime::APPLICATION_JSON);
    assert!(matches!(first.ty, GeneratedFileType::Metadata));

    let second = output.upload_queue.get(1).unwrap();
    assert_eq!(second.mime, mime::TEXT_HTML);
    assert!(matches!(second.ty, GeneratedFileType::HtmlContent));

    let third = output.upload_queue.get(2).unwrap();
    assert_eq!(third.mime, mime::TEXT_PLAIN);
    assert!(matches!(third.ty, GeneratedFileType::TextContent));

    let metadata: EmailMetadataDocument =
        serde_json::from_slice(first.bytes.as_ref()).expect("metadata should be valid json");
    assert_eq!(metadata.date, Some("2025-06-08T14:11:19Z".to_string()));
    assert_eq!(metadata.subject, Some("Test email".to_string()));
    assert_eq!(metadata.message_id, Some("test-message-id".to_string()));
    assert_eq!(
        metadata.from,
        EmailEntity {
            name: Some("Example".to_string()),
            address: Some("example@example.com".to_string())
        }
    );
    assert_eq!(
        metadata.to,
        vec![EmailEntity {
            name: Some("Example (ExampleUser)".to_string()),
            address: Some("example@example.com".to_string())
        }]
    );

    assert!(metadata.cc.is_empty());
    assert!(metadata.bcc.is_empty());
    assert!(metadata.attachments.is_empty());

    // Ensure the html content matches expectation
    let html_content = String::from_utf8_lossy(second.bytes.as_ref());
    assert_eq!(
        html_content.as_ref().replace("\r\n", "\n"),
        "<img src=\"data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR4nGNgYAAAAAMAASsJTYQAAAAASUVORK5CYII=\">\n"
    );

    // Ensure the text content matches expectation
    let text_content = String::from_utf8_lossy(third.bytes.as_ref());
    assert_eq!(text_content.as_ref().replace("\r\n", "\n"), "\n");

    let index_metadata = output
        .index_metadata
        .expect("eml file should produce index metadata");

    // Ensure page content matches expectation
    let pages = index_metadata.pages.expect("eml file should produce pages");
    assert_eq!(pages.len(), 1);

    let first_page = pages.first().unwrap();
    assert_eq!(first_page.page, 0);
    assert_eq!(first_page.content.replace("\r\n", "\n"), "\n");

    // Ensure no additional files are produced
    assert!(
        output.additional_files.is_empty(),
        "eml file without attachments should not produce additional files"
    );
}

/// Test processing a email file with an attachment
#[tokio::test]
async fn test_process_email_with_attachment() {
    // Process the file
    let output = process_sample_file(None, "sample_attachment.eml")
        .await
        .expect("eml should produce processing output");

    assert!(
        !output.encrypted,
        "file was marked as encrypted but should not be"
    );

    assert_eq!(
        output.upload_queue.len(),
        3,
        "eml file should produce 1 metadata, 1 html content, and 1 text content"
    );

    // Ensure the files match the expectations
    let first = output.upload_queue.first().unwrap();
    assert_eq!(first.mime, mime::APPLICATION_JSON);
    assert!(matches!(first.ty, GeneratedFileType::Metadata));

    let second = output.upload_queue.get(1).unwrap();
    assert_eq!(second.mime, mime::TEXT_HTML);
    assert!(matches!(second.ty, GeneratedFileType::HtmlContent));

    let third = output.upload_queue.get(2).unwrap();
    assert_eq!(third.mime, mime::TEXT_PLAIN);
    assert!(matches!(third.ty, GeneratedFileType::TextContent));

    let metadata: EmailMetadataDocument =
        serde_json::from_slice(first.bytes.as_ref()).expect("metadata should be valid json");
    assert_eq!(metadata.date, Some("2025-06-08T14:11:19Z".to_string()));
    assert_eq!(metadata.subject, Some("Test email".to_string()));
    assert_eq!(metadata.message_id, Some("test-message-id".to_string()));
    assert_eq!(
        metadata.from,
        EmailEntity {
            name: Some("Example".to_string()),
            address: Some("example@example.com".to_string())
        }
    );
    assert_eq!(
        metadata.to,
        vec![EmailEntity {
            name: Some("Example (ExampleUser)".to_string()),
            address: Some("example@example.com".to_string())
        }]
    );

    assert!(metadata.cc.is_empty());
    assert!(metadata.bcc.is_empty());
    assert_eq!(metadata.attachments.len(), 1);

    let first_attachment = metadata
        .attachments
        .first()
        .expect("should have a first attachment");

    assert_eq!(first_attachment.name, "sample.pdf");
    assert_eq!(first_attachment.mime, "application/pdf");
    assert_eq!(first_attachment.length, 25141);

    // Ensure the html content matches expectation
    let html_content = String::from_utf8_lossy(second.bytes.as_ref());
    assert_eq!(
        html_content.as_ref().replace("\r\n", "\n"),
        "<div dir=\"ltr\">Test email body<div><br></div></div>\n"
    );

    // Ensure the text content matches expectation
    let text_content = String::from_utf8_lossy(third.bytes.as_ref());
    assert_eq!(
        text_content.as_ref().replace("\r\n", "\n"),
        "Test email body\n"
    );

    let index_metadata = output
        .index_metadata
        .expect("eml file should produce index metadata");

    // Ensure page content matches expectation
    let pages = index_metadata.pages.expect("eml file should produce pages");
    assert_eq!(pages.len(), 1);

    let first_page = pages.first().unwrap();
    assert_eq!(first_page.page, 0);
    assert_eq!(
        first_page.content.replace("\r\n", "\n"),
        "Test email body\n"
    );

    // Ensure no additional files are produced
    assert_eq!(
        output.additional_files.len(),
        1,
        "eml file with attachments should produce an additional file"
    );

    let additional_file = output
        .additional_files
        .first()
        .expect("should have one additional file");

    assert_eq!(additional_file.name, "sample.pdf");
    assert_eq!(additional_file.mime, mime::APPLICATION_PDF);
}

async fn process_sample_file(
    config: Option<ProcessingConfig>,
    sample_file: &str,
) -> Option<ProcessingOutput> {
    let container = test_office_convert_server_container().await;

    // Create the processing layer
    let processing_layer =
        test_processing_layer(&container, ProcessingLayerConfig::default()).await;

    // Get the sample file
    let samples_path = Path::new("tests/samples/emails");
    let sample_file = samples_path.join(sample_file);
    let bytes = tokio::fs::read(&sample_file).await.unwrap();
    let bytes = Bytes::from(bytes);
    let mime = mime_guess::from_path(&sample_file).iter().next().unwrap();

    // Process the file
    process_file(&config, &processing_layer, bytes, &mime)
        .await
        .unwrap()
}
