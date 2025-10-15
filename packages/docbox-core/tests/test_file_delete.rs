use crate::common::{
    database::create_test_tenant_database, processing::create_processing_layer,
    search::create_test_tenant_typesense, storage::create_test_tenant_storage,
};
use docbox_core::{
    document_box::create_document_box::{CreateDocumentBox, create_document_box},
    events::TenantEventPublisher,
    files::{
        delete_file::delete_file,
        upload_file::{UploadFile, safe_upload_file},
    },
};

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

    let file = safe_upload_file(
        db.clone(),
        search.clone(),
        storage.clone(),
        events.clone(),
        processing.clone(),
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
