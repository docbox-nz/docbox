use std::str::FromStr;

use crate::common::{
    database::create_test_tenant_database, processing::create_processing_layer,
    search::create_test_tenant_typesense, storage::create_test_tenant_storage,
};
use docbox_core::{
    document_box::create_document_box::{CreateDocumentBox, create_document_box},
    events::TenantEventPublisher,
    files::upload_file::{UploadFile, upload_file},
};
use docbox_processing::{ProcessingConfig, ProcessingLayerConfig};

mod common;

/// Default limiting should ensure that an email with multiple nested layers of packing
/// should only unpack the first layer (Immediate attachments)
#[tokio::test]
async fn test_email_unpack_limiting_defaults() {
    let (_db, db) = create_test_tenant_database().await;
    let (_search, search) = create_test_tenant_typesense().await;
    let (_storage, storage) = create_test_tenant_storage().await;
    let (processing, _processing) = create_processing_layer(Default::default()).await;

    let events = TenantEventPublisher::Noop(Default::default());
    let (document_box, root) = create_document_box(
        &db,
        &events,
        CreateDocumentBox {
            scope: "test".to_string(),
            created_by: None,
        },
    )
    .await
    .unwrap();

    let nested_email_sample =
        include_str!("../../docbox-processing/tests/samples/emails/sample_attachment_nested_1.eml");

    let output = upload_file(
        &db,
        &search,
        &storage,
        &processing,
        &events,
        UploadFile {
            fixed_id: None,
            parent_id: None,
            folder_id: root.id,
            document_box: document_box.scope.clone(),
            name: "test.eml".to_string(),
            mime: mime::Mime::from_str("message/rfc822").unwrap(),
            file_bytes: nested_email_sample.into(),
            created_by: None,
            file_key: None,
            processing_config: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(output.additional_files.len(), 1);

    // All nested files unpack should have no additional uploaded files
    for file in output.additional_files {
        assert_eq!(file.additional_files.len(), 0);
    }
}

/// Increasing the limit to 2 should allow the nested email to be unpacked
#[tokio::test]
async fn test_email_unpack_limiting_increased_limit_2() {
    let (_db, db) = create_test_tenant_database().await;
    let (_search, search) = create_test_tenant_typesense().await;
    let (_storage, storage) = create_test_tenant_storage().await;
    let (processing, _processing) = create_processing_layer(ProcessingLayerConfig {
        max_unpack_iterations: Some(2),
    })
    .await;
    let events = TenantEventPublisher::Noop(Default::default());
    let (document_box, root) = create_document_box(
        &db,
        &events,
        CreateDocumentBox {
            scope: "test".to_string(),
            created_by: None,
        },
    )
    .await
    .unwrap();

    let nested_email_sample =
        include_str!("../../docbox-processing/tests/samples/emails/sample_attachment_nested_1.eml");

    let output = upload_file(
        &db,
        &search,
        &storage,
        &processing,
        &events,
        UploadFile {
            fixed_id: None,
            parent_id: None,
            folder_id: root.id,
            document_box: document_box.scope.clone(),
            name: "test.eml".to_string(),
            mime: mime::Mime::from_str("message/rfc822").unwrap(),
            file_bytes: nested_email_sample.into(),
            created_by: None,
            file_key: None,
            processing_config: Some(ProcessingConfig {
                max_unpack_iterations: Some(2),
                ..Default::default()
            }),
        },
    )
    .await
    .unwrap();

    assert_eq!(output.additional_files.len(), 1);

    let file = &output.additional_files[0];
    assert_eq!(file.additional_files.len(), 1);
}

/// Increasing the limit to 3 should allow both of the nested emails to be unpacked
#[tokio::test]
async fn test_email_unpack_limiting_increased_limit_3() {
    let (_db, db) = create_test_tenant_database().await;
    let (_search, search) = create_test_tenant_typesense().await;
    let (_storage, storage) = create_test_tenant_storage().await;
    let (processing, _processing) = create_processing_layer(ProcessingLayerConfig {
        max_unpack_iterations: Some(3),
    })
    .await;
    let events = TenantEventPublisher::Noop(Default::default());
    let (document_box, root) = create_document_box(
        &db,
        &events,
        CreateDocumentBox {
            scope: "test".to_string(),
            created_by: None,
        },
    )
    .await
    .unwrap();

    let nested_email_sample =
        include_str!("../../docbox-processing/tests/samples/emails/sample_attachment_nested_2.eml");

    let output = upload_file(
        &db,
        &search,
        &storage,
        &processing,
        &events,
        UploadFile {
            fixed_id: None,
            parent_id: None,
            folder_id: root.id,
            document_box: document_box.scope.clone(),
            name: "test.eml".to_string(),
            mime: mime::Mime::from_str("message/rfc822").unwrap(),
            file_bytes: nested_email_sample.into(),
            created_by: None,
            file_key: None,
            processing_config: Some(ProcessingConfig {
                max_unpack_iterations: Some(3),
                ..Default::default()
            }),
        },
    )
    .await
    .unwrap();

    assert_eq!(output.additional_files.len(), 1);

    let file = &output.additional_files[0];
    assert_eq!(file.additional_files.len(), 1);

    let file = &file.additional_files[0];
    assert_eq!(file.additional_files.len(), 1);
}

/// Tests that when the unpacking limit is zero that no additional files are produced
#[tokio::test]
async fn test_email_unpack_limiting_zero() {
    let (_db, db) = create_test_tenant_database().await;
    let (_search, search) = create_test_tenant_typesense().await;
    let (_storage, storage) = create_test_tenant_storage().await;
    let (processing, _processing) = create_processing_layer(ProcessingLayerConfig {
        max_unpack_iterations: Some(0),
    })
    .await;

    let events = TenantEventPublisher::Noop(Default::default());
    let (document_box, root) = create_document_box(
        &db,
        &events,
        CreateDocumentBox {
            scope: "test".to_string(),
            created_by: None,
        },
    )
    .await
    .unwrap();

    let nested_email_sample =
        include_str!("../../docbox-processing/tests/samples/emails/sample_attachment_nested_1.eml");

    let output = upload_file(
        &db,
        &search,
        &storage,
        &processing,
        &events,
        UploadFile {
            fixed_id: None,
            parent_id: None,
            folder_id: root.id,
            document_box: document_box.scope.clone(),
            name: "test.eml".to_string(),
            mime: mime::Mime::from_str("message/rfc822").unwrap(),
            file_bytes: nested_email_sample.into(),
            created_by: None,
            file_key: None,
            processing_config: None,
        },
    )
    .await
    .unwrap();

    // Should have no additional files
    assert_eq!(output.additional_files.len(), 0);
}

/// Tests that when the unpacking limit is zero on the specific upload request and not the processing layer config
/// that no additional files are produced
#[tokio::test]
async fn test_email_unpack_limiting_zero_request() {
    let (_db, db) = create_test_tenant_database().await;
    let (_search, search) = create_test_tenant_typesense().await;
    let (_storage, storage) = create_test_tenant_storage().await;
    let (processing, _processing) = create_processing_layer(ProcessingLayerConfig {
        max_unpack_iterations: None,
    })
    .await;

    let events = TenantEventPublisher::Noop(Default::default());
    let (document_box, root) = create_document_box(
        &db,
        &events,
        CreateDocumentBox {
            scope: "test".to_string(),
            created_by: None,
        },
    )
    .await
    .unwrap();

    let nested_email_sample =
        include_str!("../../docbox-processing/tests/samples/emails/sample_attachment_nested_1.eml");

    let output = upload_file(
        &db,
        &search,
        &storage,
        &processing,
        &events,
        UploadFile {
            fixed_id: None,
            parent_id: None,
            folder_id: root.id,
            document_box: document_box.scope.clone(),
            name: "test.eml".to_string(),
            mime: mime::Mime::from_str("message/rfc822").unwrap(),
            file_bytes: nested_email_sample.into(),
            created_by: None,
            file_key: None,
            processing_config: Some(ProcessingConfig {
                max_unpack_iterations: Some(0),
                ..Default::default()
            }),
        },
    )
    .await
    .unwrap();

    // Should have no additional files
    assert_eq!(output.additional_files.len(), 0);
}
