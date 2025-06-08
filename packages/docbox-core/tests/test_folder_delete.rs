use docbox_core::{
    document_box::create_document_box::{CreateDocumentBox, create_document_box},
    events::{TenantEventMessage, TenantEventPublisher, mpsc::MpscEventPublisher},
    folders::{
        create_folder::{CreateFolderData, safe_create_folder},
        delete_folder::delete_folder,
    },
};
use docbox_database::models::folder::Folder;
use docbox_search::models::SearchRequest;
use uuid::Uuid;

use crate::common::{
    database::create_test_tenant_database, search::create_test_tenant_typesense,
    storage::create_test_tenant_storage,
};

mod common;

/// Tests that a folder can be deleted successfully
#[tokio::test]
async fn test_delete_folder_success() {
    let (_container_db, db) = create_test_tenant_database().await;
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

    // Consume creation event
    _ = events_rx.recv().await.unwrap();

    let folder = safe_create_folder(
        &db,
        search.clone(),
        &events,
        CreateFolderData {
            folder: root,
            name: "Test Folder".to_string(),
            created_by: None,
        },
    )
    .await
    .unwrap();

    // Consume creation event
    _ = events_rx.recv().await.unwrap();

    // Ensure the correct data was inserted
    assert_eq!(folder.name, "Test Folder");
    assert_eq!(folder.created_by, None);

    let folder_id = folder.id;

    // Delete the folder
    delete_folder(&db, &storage, &search, &events, folder)
        .await
        .unwrap();

    // Expect deletion event
    let event = events_rx.recv().await.unwrap();
    assert!(matches!(
        event,
        TenantEventMessage::FolderDeleted(deleted) if deleted.data.id == folder_id
    ));

    // Ensure the folder cannot be found
    {
        let has_folder = Folder::find_by_id(&db, &document_box.scope, folder_id)
            .await
            .unwrap()
            .is_some();
        assert!(!has_folder);
    }

    // Ensure the name is correctly removed from the index and is not searchable
    {
        let request = SearchRequest {
            query: Some("Test Folder".to_string()),
            include_name: true,
            ..Default::default()
        };

        let result = search
            .search_index(&["test".to_string()], request, None)
            .await
            .unwrap();

        assert_eq!(result.total_hits, 0);
        assert!(result.results.is_empty());
    }
}

/// Tests that a folder can be deleted successfully and that
/// its children will also be deleted
#[tokio::test]
async fn test_delete_folder_children_success() {
    let (_container_db, db) = create_test_tenant_database().await;
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

    // Consume creation event
    _ = events_rx.recv().await.unwrap();

    let folder = safe_create_folder(
        &db,
        search.clone(),
        &events,
        CreateFolderData {
            folder: root,
            name: "Test Folder".to_string(),
            created_by: None,
        },
    )
    .await
    .unwrap();

    // Consume creation event
    _ = events_rx.recv().await.unwrap();

    let sub_folder = safe_create_folder(
        &db,
        search.clone(),
        &events,
        CreateFolderData {
            folder: folder.clone(),
            name: "Sub Folder".to_string(),
            created_by: None,
        },
    )
    .await
    .unwrap();

    // Consume creation event
    _ = events_rx.recv().await.unwrap();

    let folder_id = folder.id;

    // Delete the folder
    delete_folder(&db, &storage, &search, &events, folder)
        .await
        .unwrap();

    // Expect sub folder deletion event
    let event = events_rx.recv().await.unwrap();
    assert!(matches!(
        event,
        TenantEventMessage::FolderDeleted(deleted) if deleted.data.id == sub_folder.id
    ));

    // Expect deletion event for main folder
    let event = events_rx.recv().await.unwrap();
    assert!(matches!(
        event,
        TenantEventMessage::FolderDeleted(deleted) if deleted.data.id == folder_id
    ));

    // Ensure the folder cannot be found
    {
        let has_sub_folder = Folder::find_by_id(&db, &document_box.scope, sub_folder.id)
            .await
            .unwrap()
            .is_some();
        assert!(!has_sub_folder);
    }

    // Ensure the name is correctly removed from the index and is not searchable
    {
        let request = SearchRequest {
            query: Some("Sub Folder".to_string()),
            include_name: true,
            ..Default::default()
        };

        let result = search
            .search_index(&["test".to_string()], request, None)
            .await
            .unwrap();

        assert_eq!(result.total_hits, 0);
        assert!(result.results.is_empty());
    }
}

/// Tests that attempt to delete a non-existent folder should not
/// produce any events
#[tokio::test]
async fn test_delete_unknown_folder() {
    let (_container_db, db) = create_test_tenant_database().await;
    let (_container_search, search) = create_test_tenant_typesense().await;
    let (_container_storage, storage) = create_test_tenant_storage().await;
    let (events, mut events_rx) = MpscEventPublisher::new();
    let events = TenantEventPublisher::Mpsc(events);
    let (_document_box, _root) = create_document_box(
        &db,
        &events,
        CreateDocumentBox {
            scope: "test".to_string(),
            created_by: None,
        },
    )
    .await
    .unwrap();

    // Consume creation event
    _ = events_rx.recv().await.unwrap();

    let fake_folder = Folder {
        id: Uuid::nil(),
        name: Default::default(),
        document_box: Default::default(),
        folder_id: Default::default(),
        created_at: Default::default(),
        created_by: Default::default(),
    };

    // Delete the folder
    delete_folder(&db, &storage, &search, &events, fake_folder)
        .await
        .unwrap();

    // Should have nothing to consume
    assert!(events_rx.try_recv().is_err());
}
