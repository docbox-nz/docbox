use std::str::FromStr;

use bytes::Bytes;
use docbox_database::models::generated_file::GeneratedFileType;
use docbox_processing::{
    ProcessingLayerConfig, process_file,
    text::{is_application_file, is_text_file, process_text},
};
use mime::Mime;

use crate::common::processing::noop_processing_layer;

mod common;

fn mime(value: &str) -> Mime {
    Mime::from_str(value).unwrap()
}

async fn process_with_mime(
    contents: &[u8],
    mime_type: &str,
) -> Option<docbox_processing::ProcessingOutput> {
    process_file(
        &None,
        &noop_processing_layer(ProcessingLayerConfig {
            process_application_files: Some(true),
            ..Default::default()
        }),
        Bytes::copy_from_slice(contents),
        &mime(mime_type),
    )
    .await
    .unwrap()
}

fn assert_text_content(output: &docbox_processing::ProcessingOutput, expected: &str) {
    assert!(!output.encrypted, "text files are not encrypted");
    assert!(
        output.additional_files.is_empty(),
        "text files should not produce additional files"
    );

    assert_eq!(
        output.upload_queue.len(),
        1,
        "text file should produce 1 text content file"
    );

    let first = output.upload_queue.first().unwrap();
    assert_eq!(first.mime, mime::TEXT_PLAIN);
    assert!(matches!(first.ty, GeneratedFileType::TextContent));
    assert_eq!(String::from_utf8_lossy(first.bytes.as_ref()), expected);

    let pages = output
        .index_metadata
        .as_ref()
        .expect("text file should produce index metadata")
        .pages
        .as_ref()
        .expect("text file should produce pages");

    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0].page, 0);
    assert_eq!(pages[0].content, expected);
}

#[test]
fn test_is_text_mime() {
    assert!(is_text_file(&mime("text/plain")));
    assert!(is_text_file(&mime("text/markdown")));
    assert!(is_text_file(&mime("text/x-markdown")));
    assert!(is_text_file(&mime("text/csv")));
    assert!(is_text_file(&mime("text/xml")));

    assert!(!is_text_file(&mime("application/pdf")));
    assert!(!is_text_file(&mime("application/octet-stream")));
    assert!(!is_text_file(&mime("image/png")));
}

#[test]
fn test_is_application_mime() {
    assert!(is_application_file(&mime("application/json")));
    assert!(is_application_file(&mime("application/ld+json")));
    assert!(is_application_file(&mime("application/xml")));
    assert!(is_application_file(&mime("application/atom+xml")));
    assert!(!is_application_file(&mime("application/pdf")));
    assert!(!is_application_file(&mime("application/octet-stream")));
    assert!(!is_application_file(&mime("image/png")));
}

#[test]
fn test_process_text_plain_sample() {
    let bytes = include_bytes!("samples/documents/sample.txt");
    let output = process_text(bytes);
    let expected = String::from_utf8_lossy(bytes);
    assert_text_content(&output, expected.as_ref());
}

#[test]
fn test_process_text_markdown() {
    let source = "# Heading\n\nSome **markdown** content.\n";
    let output = process_text(source.as_bytes());
    assert_text_content(&output, source);
}

#[test]
fn test_process_text_csv() {
    let source = "name,role\nAda,Engineer\n";
    let output = process_text(source.as_bytes());
    assert_text_content(&output, source);
}

#[test]
fn test_process_text_json() {
    let source = r#"{"name":"Ada","role":"Engineer"}"#;
    let output = process_text(source.as_bytes());
    assert_text_content(&output, source);
}

#[test]
fn test_process_text_xml() {
    let source = "<person><name>Ada</name></person>\n";
    let output = process_text(source.as_bytes());
    assert_text_content(&output, source);
}

#[test]
fn test_process_text_strips_utf8_bom() {
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(b"hello");

    let output = process_text(&bytes);
    assert_text_content(&output, "hello");
}

#[test]
fn test_process_text_empty() {
    let output = process_text(b"");
    assert_text_content(&output, "");
}

#[tokio::test]
async fn test_process_file_text_plain() {
    let bytes = include_bytes!("samples/documents/sample.txt");
    let output = process_with_mime(bytes, "text/plain")
        .await
        .expect("text/plain should produce processing output");
    assert_text_content(&output, String::from_utf8_lossy(bytes).as_ref());
}

#[tokio::test]
async fn test_process_file_markdown() {
    let source = "# Title\n";
    let output = process_with_mime(source.as_bytes(), "text/markdown")
        .await
        .expect("text/markdown should produce processing output");
    assert_text_content(&output, source);

    let output = process_with_mime(source.as_bytes(), "text/x-markdown")
        .await
        .expect("text/x-markdown should produce processing output");
    assert_text_content(&output, source);
}

#[tokio::test]
async fn test_process_file_csv() {
    let source = "a,b\n1,2\n";
    let output = process_with_mime(source.as_bytes(), "text/csv")
        .await
        .expect("text/csv should produce processing output");
    assert_text_content(&output, source);
}

#[tokio::test]
async fn test_process_file_json() {
    let source = r#"{"ok":true}"#;
    let output = process_with_mime(source.as_bytes(), "application/json")
        .await
        .expect("application/json should produce processing output");
    assert_text_content(&output, source);

    let output = process_with_mime(source.as_bytes(), "application/ld+json")
        .await
        .expect("application/ld+json should produce processing output");
    assert_text_content(&output, source);
}

#[tokio::test]
async fn test_process_file_xml() {
    let source = "<root/>";
    let output = process_with_mime(source.as_bytes(), "application/xml")
        .await
        .expect("application/xml should produce processing output");
    assert_text_content(&output, source);

    let output = process_with_mime(source.as_bytes(), "text/xml")
        .await
        .expect("text/xml should produce processing output");
    assert_text_content(&output, source);
}

#[tokio::test]
async fn test_process_file_unrelated_mime() {
    let output = process_with_mime(b"not-a-document", "application/octet-stream").await;
    assert!(
        output.is_none(),
        "unrelated mime types should not be processed"
    );
}
