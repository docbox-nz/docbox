use crate::common::processing::create_processing_layer;
use bytes::Bytes;
use docbox_core::processing::{ProcessingError, ProcessingOutput, process_file};
use docbox_database::models::generated_file::GeneratedFileType;
use std::path::Path;

mod common;

/// Test processing a PDF file
#[tokio::test]
async fn test_process_pdf() {
    // Process the file
    let output = process_sample_file("sample.pdf")
        .await
        .expect("pdf should produce processing output");

    assert!(
        !output.encrypted,
        "File was marked as encrypted but should not be"
    );

    assert_eq!(
        output.upload_queue.len(),
        4,
        "PDF file should produce 3 images and 1 text file"
    );

    // Ensure the files match the expectations
    let first = output.upload_queue.first().unwrap();
    assert_eq!(first.mime, mime::IMAGE_JPEG);
    assert!(matches!(first.ty, GeneratedFileType::CoverPage));

    let second = output.upload_queue.get(1).unwrap();
    assert_eq!(second.mime, mime::IMAGE_JPEG);
    assert!(matches!(second.ty, GeneratedFileType::LargeThumbnail));

    let third = output.upload_queue.get(2).unwrap();
    assert_eq!(third.mime, mime::IMAGE_JPEG);
    assert!(matches!(third.ty, GeneratedFileType::SmallThumbnail));

    let forth = output.upload_queue.get(3).unwrap();
    assert_eq!(forth.mime, mime::TEXT_PLAIN);
    assert!(matches!(forth.ty, GeneratedFileType::TextContent));

    // Ensure the text content matches expectation
    let text_content = String::from_utf8_lossy(forth.bytes.as_ref());
    assert_eq!(
        text_content.as_ref().replace("\r\n", "\n"),
        "Sample document\nThis is a second line\n\n\u{c}This is the second page\n\n\u{c}"
    );

    let index_metadata = output
        .index_metadata
        .expect("pdf file should produce index metadata");

    // Ensure page content matches expectation
    let pages = index_metadata.pages.expect("pdf file should produce pages");
    assert_eq!(pages.len(), 3);

    let first_page = pages.first().unwrap();
    assert_eq!(first_page.page, 0);
    assert_eq!(
        first_page.content.replace("\r\n", "\n"),
        "Sample document\nThis is a second line\n\n"
    );

    let second_page = pages.get(1).unwrap();
    assert_eq!(second_page.page, 1);
    assert_eq!(
        second_page.content.replace("\r\n", "\n"),
        "This is the second page\n\n"
    );

    let third_page = pages.get(2).unwrap();
    assert_eq!(third_page.page, 2);
    assert_eq!(third_page.content, "");

    // Ensure no additional files are produced
    assert!(
        output.additional_files.is_empty(),
        "PDF file should not produce additional files"
    );
}

/// Test processing a Word Document (.docx) file
#[tokio::test]
async fn test_process_docx() {
    test_process_document("sample.docx").await;
}

/// Test processing a Rich Text Format (.rtf) file
#[tokio::test]
async fn test_process_rtf() {
    test_process_document("sample.rtf").await;
}

/// Test processing a OpenDocument Text file (.odt) file
#[tokio::test]
async fn test_process_odt() {
    test_process_document("sample.odt").await;
}

/// Test processing a Word Template (.dotx) file
#[tokio::test]
async fn test_process_dotx() {
    test_process_document("sample.dotx").await;
}

/// Test processing a Word 97-2003 Template (.dot) file
#[tokio::test]
async fn test_process_dot() {
    test_process_document("sample.dot").await;
}

/// Test processing a Word 97-2003 Document (.doc) file
#[tokio::test]
async fn test_process_doc() {
    test_process_document("sample.doc").await;
}

/// Test processing a Word Macro-Enabled Template (.dotm) file
#[tokio::test]
async fn test_process_dotm() {
    test_process_document("sample.dotm").await;
}

/// Test processing a encrypted PDF file
#[tokio::test]
async fn test_process_pdf_encrypted() {
    test_process_encrypted("sample_encrypted.pdf").await;
}

/// Test processing a encrypted Word Document (.docx) file
#[tokio::test]
async fn test_process_docx_encrypted() {
    test_process_encrypted("sample_encrypted.docx").await;
}

/// Test processing a encrypted Word 97-2003 Document (.doc) file
#[tokio::test]
async fn test_process_doc_encrypted() {
    test_process_encrypted("sample_encrypted.doc").await;
}

/// Test processing a corrupted Word Document (.docx) file
#[tokio::test]
async fn test_process_docx_corrupted() {
    // Create the processing layer
    let (processing_layer, _container) = create_processing_layer().await;

    // Get the sample file
    let samples_path = Path::new("tests/samples/documents");
    let sample_file = samples_path.join("sample_corrupted.docx");
    let bytes = tokio::fs::read(&sample_file).await.unwrap();
    let bytes = Bytes::from(bytes);
    let mime = mime_guess::from_path(&sample_file).iter().next().unwrap();

    // Process the file
    let output = process_file(&None, &processing_layer, bytes, &mime)
        .await
        .unwrap_err();

    assert!(
        matches!(output, ProcessingError::MalformedFile(_),),
        "corrupted file should produce a malformed document error got {output:?}"
    );
}

/// Test processing a Excel Workbook (.xlsx) file
#[tokio::test]
async fn test_process_xlsx() {
    test_process_workbook("sample.xlsx").await;
}

/// Test processing a Excel Binary Workbook (.xlsb) file
#[tokio::test]
async fn test_process_xlsb() {
    test_process_workbook("sample.xlsb").await;
}

/// Test processing a Excel 97-2003 Workbook (.xls) file
#[tokio::test]
async fn test_process_xls() {
    test_process_workbook("sample.xls").await;
}

/// Test processing a Excel Macro-Enabled Workbook (.xlsm) file
#[tokio::test]
async fn test_process_xlsm() {
    test_process_workbook("sample.xlsm").await;
}

/// Test processing a Excel 97-2003 Template (.xlt) file
#[tokio::test]
async fn test_process_xlt() {
    test_process_workbook("sample.xlt").await;
}

/// Test processing a Excel Macro-Enabled Template (.xltm) file
#[tokio::test]
async fn test_process_xltm() {
    test_process_workbook("sample.xltm").await;
}

/// Test processing a Excel Template (.xltx) file
#[tokio::test]
async fn test_process_xltx() {
    test_process_workbook("sample.xltx").await;
}

/// Test processing a OpenDocument Spreadsheet (.ods) file
#[tokio::test]
async fn test_process_ods() {
    let output = process_sample_file("sample.ods")
        .await
        .expect("office file should produce output");

    assert!(
        !output.encrypted,
        "file was marked as encrypted but should not be"
    );

    assert_eq!(
        output.upload_queue.len(),
        5,
        "office file should produce 1 pdf, 3 images and 1 text file"
    );

    // Ensure the files match the expectations
    let first = output.upload_queue.first().unwrap();
    assert_eq!(first.mime, mime::IMAGE_JPEG);
    assert!(matches!(first.ty, GeneratedFileType::CoverPage));

    let second = output.upload_queue.get(1).unwrap();
    assert_eq!(second.mime, mime::IMAGE_JPEG);
    assert!(matches!(second.ty, GeneratedFileType::LargeThumbnail));

    let third = output.upload_queue.get(2).unwrap();
    assert_eq!(third.mime, mime::IMAGE_JPEG);
    assert!(matches!(third.ty, GeneratedFileType::SmallThumbnail));

    let forth = output.upload_queue.get(3).unwrap();
    assert_eq!(forth.mime, mime::TEXT_PLAIN);
    assert!(matches!(forth.ty, GeneratedFileType::TextContent));

    let fifth = output.upload_queue.get(4).unwrap();
    assert_eq!(fifth.mime, mime::APPLICATION_PDF);
    assert!(matches!(fifth.ty, GeneratedFileType::Pdf));

    // Ensure the text content matches expectation
    let text_content = String::from_utf8_lossy(forth.bytes.as_ref());
    assert_eq!(
        text_content.as_ref().replace("\r\n", "\n"),
        "Sample Sample 1Sample 2\n\n\u{c}"
    );

    let index_metadata = output
        .index_metadata
        .as_ref()
        .expect("office file should produce index metadata");

    // Ensure page content matches expectation
    let pages = index_metadata
        .pages
        .as_ref()
        .expect("office file should produce pages");
    assert_eq!(pages.len(), 2);

    let first_page = pages.first().unwrap();
    assert_eq!(first_page.page, 0);
    assert_eq!(
        first_page.content.replace("\r\n", "\n"),
        "Sample Sample 1Sample 2\n\n"
    );

    let second_page = pages.get(1).unwrap();
    assert_eq!(second_page.page, 1);
    assert_eq!(second_page.content, "");

    // Ensure no additional files are produced
    assert!(
        output.additional_files.is_empty(),
        "office file should not produce additional files"
    );
}

/// Test processing a encrypted Excel Workbook (.xlsx) file
#[tokio::test]
async fn test_process_xlsx_encrypted() {
    test_process_encrypted("sample_encrypted.xlsx").await;
}

/// Test processing a encrypted Excel 97-2003 Workbook (.xls) file
#[tokio::test]
async fn test_process_xls_encrypted() {
    test_process_encrypted("sample_encrypted.xls").await;
}

async fn process_sample_file(sample_file: &str) -> Option<ProcessingOutput> {
    // Create the processing layer
    let (processing_layer, _container) = create_processing_layer().await;

    // Get the sample file
    let samples_path = Path::new("tests/samples/documents");
    let sample_file = samples_path.join(sample_file);
    let bytes = tokio::fs::read(&sample_file).await.unwrap();
    let bytes = Bytes::from(bytes);
    let mime = mime_guess::from_path(&sample_file).iter().next().unwrap();

    // Process the file
    process_file(&None, &processing_layer, bytes, &mime)
        .await
        .unwrap()
}

async fn test_process_encrypted(sample_file: &str) {
    let output = process_sample_file(sample_file)
        .await
        .expect("office file should produce output");

    assert!(
        output.encrypted,
        "File was not marked as encrypted but should be"
    );
    assert!(
        output.upload_queue.is_empty(),
        "Encrypted file should not produce uploads"
    );
    assert!(
        output.index_metadata.is_none(),
        "Encrypted file should not produce index metadata"
    );
    assert!(
        output.additional_files.is_empty(),
        "Encrypted file should not produce additional files"
    );
}

async fn test_process_workbook(sample_file: &str) {
    let output = process_sample_file(sample_file)
        .await
        .expect("office file should produce output");
    validate_workbook_output(&output);
}

async fn test_process_document(sample_file: &str) {
    let output = process_sample_file(sample_file)
        .await
        .expect("office file should produce output");

    validate_document_output(&output);
}

/// Validates the expected output for all office document formats
fn validate_document_output(output: &ProcessingOutput) {
    assert!(
        !output.encrypted,
        "file was marked as encrypted but should not be"
    );

    assert_eq!(
        output.upload_queue.len(),
        5,
        "office file should produce 1 pdf, 3 images and 1 text file"
    );

    // Ensure the files match the expectations
    let first = output.upload_queue.first().unwrap();
    assert_eq!(first.mime, mime::IMAGE_JPEG);
    assert!(matches!(first.ty, GeneratedFileType::CoverPage));

    let second = output.upload_queue.get(1).unwrap();
    assert_eq!(second.mime, mime::IMAGE_JPEG);
    assert!(matches!(second.ty, GeneratedFileType::LargeThumbnail));

    let third = output.upload_queue.get(2).unwrap();
    assert_eq!(third.mime, mime::IMAGE_JPEG);
    assert!(matches!(third.ty, GeneratedFileType::SmallThumbnail));

    let forth = output.upload_queue.get(3).unwrap();
    assert_eq!(forth.mime, mime::TEXT_PLAIN);
    assert!(matches!(forth.ty, GeneratedFileType::TextContent));

    let fifth = output.upload_queue.get(4).unwrap();
    assert_eq!(fifth.mime, mime::APPLICATION_PDF);
    assert!(matches!(fifth.ty, GeneratedFileType::Pdf));

    // Ensure the text content matches expectation
    let text_content = String::from_utf8_lossy(forth.bytes.as_ref());
    assert_eq!(
        text_content.as_ref().replace("\r\n", "\n"),
        "Sample document\nThis is a second line\n\n\u{c}This is the second page\n\n\u{c}"
    );

    let index_metadata = output
        .index_metadata
        .as_ref()
        .expect("office file should produce index metadata");

    // Ensure page content matches expectation
    let pages = index_metadata
        .pages
        .as_ref()
        .expect("office file should produce pages");
    assert_eq!(pages.len(), 3);

    let first_page = pages.first().unwrap();
    assert_eq!(first_page.page, 0);
    assert_eq!(
        first_page.content.replace("\r\n", "\n"),
        "Sample document\nThis is a second line\n\n"
    );

    let second_page = pages.get(1).unwrap();
    assert_eq!(second_page.page, 1);
    assert_eq!(
        second_page.content.replace("\r\n", "\n"),
        "This is the second page\n\n"
    );

    let third_page = pages.get(2).unwrap();
    assert_eq!(third_page.page, 2);
    assert_eq!(third_page.content, "");

    // Ensure no additional files are produced
    assert!(
        output.additional_files.is_empty(),
        "office file should not produce additional files"
    );
}

/// Validates the expected output for all office workbook formats
fn validate_workbook_output(output: &ProcessingOutput) {
    assert!(
        !output.encrypted,
        "file was marked as encrypted but should not be"
    );

    assert_eq!(
        output.upload_queue.len(),
        5,
        "office file should produce 1 pdf, 3 images and 1 text file"
    );

    // Ensure the files match the expectations
    let first = output.upload_queue.first().unwrap();
    assert_eq!(first.mime, mime::IMAGE_JPEG);
    assert!(matches!(first.ty, GeneratedFileType::CoverPage));

    let second = output.upload_queue.get(1).unwrap();
    assert_eq!(second.mime, mime::IMAGE_JPEG);
    assert!(matches!(second.ty, GeneratedFileType::LargeThumbnail));

    let third = output.upload_queue.get(2).unwrap();
    assert_eq!(third.mime, mime::IMAGE_JPEG);
    assert!(matches!(third.ty, GeneratedFileType::SmallThumbnail));

    let forth = output.upload_queue.get(3).unwrap();
    assert_eq!(forth.mime, mime::TEXT_PLAIN);
    assert!(matches!(forth.ty, GeneratedFileType::TextContent));

    let fifth = output.upload_queue.get(4).unwrap();
    assert_eq!(fifth.mime, mime::APPLICATION_PDF);
    assert!(matches!(fifth.ty, GeneratedFileType::Pdf));

    // Ensure the text content matches expectation
    let text_content = String::from_utf8_lossy(forth.bytes.as_ref());
    assert_eq!(
        text_content.as_ref().replace("\r\n", "\n"),
        "Sample\n\nSample 1 Sample 2\n\n\u{c}"
    );

    let index_metadata = output
        .index_metadata
        .as_ref()
        .expect("office file should produce index metadata");

    // Ensure page content matches expectation
    let pages = index_metadata
        .pages
        .as_ref()
        .expect("office file should produce pages");
    assert_eq!(pages.len(), 2);

    let first_page = pages.first().unwrap();
    assert_eq!(first_page.page, 0);
    assert_eq!(
        first_page.content.replace("\r\n", "\n"),
        "Sample\n\nSample 1 Sample 2\n\n"
    );

    let second_page = pages.get(1).unwrap();
    assert_eq!(second_page.page, 1);
    assert_eq!(second_page.content, "");

    // Ensure no additional files are produced
    assert!(
        output.additional_files.is_empty(),
        "office file should not produce additional files"
    );
}
