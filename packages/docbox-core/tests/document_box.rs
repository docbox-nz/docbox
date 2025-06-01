use common::database::create_test_tenant_database;
use docbox_core::{
    document_box::create_document_box::{create_document_box, CreateDocumentBox},
    events::{noop::NoopEventPublisher, TenantEventPublisher},
};

mod common;

/// Creating a document box that doesn't exist should succeed
#[tokio::test]
async fn test_create_document_box_success() {
    let (_container, db) = create_test_tenant_database().await;
    let events = TenantEventPublisher::Noop(NoopEventPublisher);
    create_document_box(
        &db,
        &events,
        CreateDocumentBox {
            scope: "test".to_string(),
            created_by: None,
        },
    )
    .await
    .unwrap();
}

/// Attempting to create a document box with a duplicate scope should
/// produce an error
#[tokio::test]
async fn test_create_document_box_duplicate_scope() {
    let (_container, db) = create_test_tenant_database().await;
    let events = TenantEventPublisher::Noop(NoopEventPublisher);

    // Should succeed
    create_document_box(
        &db,
        &events,
        CreateDocumentBox {
            scope: "test".to_string(),
            created_by: None,
        },
    )
    .await
    .unwrap();

    // Should fail
    create_document_box(
        &db,
        &events,
        CreateDocumentBox {
            scope: "test".to_string(),
            created_by: None,
        },
    )
    .await
    .unwrap_err();
}
