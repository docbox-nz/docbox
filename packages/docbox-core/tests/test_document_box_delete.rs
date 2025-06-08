use crate::common::{search::create_test_tenant_typesense, storage::create_test_tenant_storage};
use common::database::create_test_tenant_database;
use docbox_core::{
    document_box::{
        create_document_box::{CreateDocumentBox, create_document_box},
        delete_document_box::{DeleteDocumentBoxError, delete_document_box},
    },
    events::{TenantEventMessage, TenantEventPublisher, mpsc::MpscEventPublisher},
    folders::create_folder::{CreateFolderData, safe_create_folder},
};

mod common;

#[tokio::test]
async fn test_delete_document_box() {
    let (_container, db) = create_test_tenant_database().await;
    let (_container_search, search) = create_test_tenant_typesense().await;
    let (_container_storage, storage) = create_test_tenant_storage().await;

    let (events, mut events_rx) = MpscEventPublisher::new();
    let events = TenantEventPublisher::Mpsc(events);
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

    _ = events_rx.recv().await.unwrap();

    delete_document_box(&db, &search, &storage, &events, "test".to_string())
        .await
        .unwrap();

    // Should be notified the root was deleted
    let event = events_rx.recv().await.unwrap();
    assert!(matches!(
        event,
        TenantEventMessage::FolderDeleted(deleted) if deleted.data.id == root.id
    ));

    // Should be notified the document box was deleted
    let event = events_rx.recv().await.unwrap();
    assert!(matches!(
        event,
        TenantEventMessage::DocumentBoxDeleted(deleted) if deleted.scope == document_box.scope
    ));
}

#[tokio::test]
async fn test_delete_unknown_document_box() {
    let (_container, db) = create_test_tenant_database().await;
    let (_container_search, search) = create_test_tenant_typesense().await;
    let (_container_storage, storage) = create_test_tenant_storage().await;

    let (events, mut events_rx) = MpscEventPublisher::new();
    let events = TenantEventPublisher::Mpsc(events);

    let err = delete_document_box(&db, &search, &storage, &events, "test".to_string())
        .await
        .unwrap_err();

    assert!(
        matches!(err, DeleteDocumentBoxError::UnknownScope),
        "should get unknown scope error for missing scope"
    );

    // Should have nothing to consume
    assert!(events_rx.try_recv().is_err());
}

/// Tests that deleting a document box also deletes the root and any children
/// within the document box
#[tokio::test]
async fn test_delete_document_box_deletes_children() {
    let (_container, db) = create_test_tenant_database().await;
    let (_container_search, search) = create_test_tenant_typesense().await;
    let (_container_storage, storage) = create_test_tenant_storage().await;

    let (events, mut events_rx) = MpscEventPublisher::new();
    let events = TenantEventPublisher::Mpsc(events);
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

    _ = events_rx.recv().await.unwrap();

    let created_folder = safe_create_folder(
        &db,
        search.clone(),
        &events,
        CreateFolderData {
            folder: root.clone(),
            name: "Test Folder".to_string(),
            created_by: None,
        },
    )
    .await
    .unwrap();

    _ = events_rx.recv().await.unwrap();

    delete_document_box(&db, &search, &storage, &events, "test".to_string())
        .await
        .unwrap();

    // Should be notified the child folder was deleted
    let event = events_rx.recv().await.unwrap();
    assert!(matches!(
        event,
        TenantEventMessage::FolderDeleted(deleted) if deleted.data.id == created_folder.id
    ));

    // Should be notified the root was deleted
    let event = events_rx.recv().await.unwrap();
    assert!(matches!(
        event,
        TenantEventMessage::FolderDeleted(deleted) if deleted.data.id == root.id
    ));

    // Should be notified the document box was deleted
    let event = events_rx.recv().await.unwrap();
    assert!(matches!(
        event,
        TenantEventMessage::DocumentBoxDeleted(deleted) if deleted.scope == document_box.scope
    ));
}
