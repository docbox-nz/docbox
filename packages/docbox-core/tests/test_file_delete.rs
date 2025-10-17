use crate::common::{
    database::create_test_tenant_database, processing::create_processing_layer,
    search::create_test_tenant_typesense, storage::create_test_tenant_storage,
};
use docbox_core::{
    document_box::create_document_box::{CreateDocumentBox, create_document_box},
    events::{TenantEventPublisher, mpsc::MpscEventPublisher},
    files::{
        delete_file::delete_file,
        upload_file::{UploadFile, upload_file},
    },
};
use docbox_database::models::file::File;
use uuid::Uuid;

mod common;

/// Tests that a file can be deleted
#[tokio::test]
async fn test_file_delete_success() {
    let (_db, db) = create_test_tenant_database().await;
    let (_search, search) = create_test_tenant_typesense().await;
    let (_storage, storage) = create_test_tenant_storage().await;
    let (processing, _processing) = create_processing_layer().await;

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

    let file = upload_file(
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
            name: "test.txt".to_string(),
            mime: mime::TEXT_PLAIN,
            file_bytes: "test".into(),
            created_by: None,
            file_key: None,
            processing_config: None,
        },
    )
    .await
    .unwrap();

    let file = file.file;

    delete_file(&db, &storage, &search, &events, file, document_box.scope)
        .await
        .unwrap();
}

/// Tests that deleting a file that doesn't exist should not produce an event
#[tokio::test]
async fn test_file_delete_unknown_no_event() {
    let (_db, db) = create_test_tenant_database().await;
    let (_search, search) = create_test_tenant_typesense().await;
    let (_storage, storage) = create_test_tenant_storage().await;

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

    let fake_file = File {
        id: Uuid::new_v4(),
        name: Default::default(),
        mime: "text/plain".to_string(),
        folder_id: root.id,
        hash: "".to_string(),
        size: 1,
        encrypted: false,
        pinned: Default::default(),
        file_key: "file.txt".to_string(),
        created_at: Default::default(),
        created_by: Default::default(),
        parent_id: None,
    };

    // Delete the fake file
    delete_file(
        &db,
        &storage,
        &search,
        &events,
        fake_file,
        document_box.scope,
    )
    .await
    .unwrap();

    // Should have nothing to consume
    assert!(events_rx.try_recv().is_err());
}
