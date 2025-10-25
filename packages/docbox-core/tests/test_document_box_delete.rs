use crate::common::{
    database::test_tenant_db, minio::test_tenant_storage, tenant::test_tenant,
    typesense::test_tenant_search,
};
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
    let tenant = test_tenant();

    let (db, _db_container) = test_tenant_db().await;
    let (search, _search_container) = test_tenant_search(&tenant).await;
    let (storage, _storage_container) = test_tenant_storage(&tenant).await;

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

    delete_document_box(&db, &search, &storage, &events, "test")
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
    let tenant = test_tenant();

    let (db, _db_container) = test_tenant_db().await;
    let (search, _search_container) = test_tenant_search(&tenant).await;
    let (storage, _storage_container) = test_tenant_storage(&tenant).await;

    let (events, mut events_rx) = MpscEventPublisher::new();
    let events = TenantEventPublisher::Mpsc(events);

    let err = delete_document_box(&db, &search, &storage, &events, "test")
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
    let tenant = test_tenant();

    let (db, _db_container) = test_tenant_db().await;
    let (search, _search_container) = test_tenant_search(&tenant).await;
    let (storage, _storage_container) = test_tenant_storage(&tenant).await;

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

    delete_document_box(&db, &search, &storage, &events, "test")
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
