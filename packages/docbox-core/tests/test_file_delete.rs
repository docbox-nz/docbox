use crate::common::{
    database::test_tenant_db,
    minio::test_tenant_storage,
    processing::{test_office_convert_server_container, test_processing_layer},
    tenant::test_tenant,
    typesense::test_tenant_search,
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
use docbox_processing::ProcessingLayerConfig;
use uuid::Uuid;

mod common;

/// Tests that a file can be deleted
#[tokio::test]
async fn test_file_delete_success() {
    let tenant = test_tenant();

    let (db, _db_container) = test_tenant_db().await;
    let (search, _search_container) = test_tenant_search(&tenant).await;
    let (storage, _storage_container) = test_tenant_storage(&tenant).await;

    let converter_container = test_office_convert_server_container().await;
    let processing =
        test_processing_layer(&converter_container, ProcessingLayerConfig::default()).await;

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
