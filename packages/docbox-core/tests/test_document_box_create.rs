use crate::common::database::test_tenant_db;
use docbox_core::{
    document_box::create_document_box::{
        CreateDocumentBox, CreateDocumentBoxError, create_document_box,
    },
    events::{
        TenantEventMessage, TenantEventPublisher, mpsc::MpscEventPublisher,
        noop::NoopEventPublisher,
    },
};

mod common;

/// Creating a document box that doesn't exist should succeed
#[tokio::test]
async fn test_create_document_box_success() {
    let (db, _db_container) = test_tenant_db().await;

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

/// Creating a document box should emit a creation event
#[tokio::test]
async fn test_create_document_box_success_event() {
    let (db, _db_container) = test_tenant_db().await;

    let (events, mut events_rx) = MpscEventPublisher::new();
    let events = TenantEventPublisher::Mpsc(events);
    let (document_box, _root) = create_document_box(
        &db,
        &events,
        CreateDocumentBox {
            scope: "test".to_string(),
            created_by: None,
        },
    )
    .await
    .unwrap();

    let event = events_rx.recv().await.unwrap();
    assert!(matches!(
        event,
        TenantEventMessage::DocumentBoxCreated(created) if created.scope == document_box.scope
    ));
}

/// Attempting to create a document box with a duplicate scope should
/// produce an error
#[tokio::test]
async fn test_create_document_box_duplicate_scope() {
    let (db, _db_container) = test_tenant_db().await;

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
    let error = create_document_box(
        &db,
        &events,
        CreateDocumentBox {
            scope: "test".to_string(),
            created_by: None,
        },
    )
    .await
    .unwrap_err();

    assert!(matches!(error, CreateDocumentBoxError::ScopeAlreadyExists))
}
