use crate::common::{
    database::test_tenant_db,
    minio::test_tenant_storage,
    processing::{test_office_convert_server_container, test_processing_layer},
    tenant::test_tenant,
    typesense::test_tenant_search,
};
use docbox_core::{
    document_box::create_document_box::{CreateDocumentBox, create_document_box},
    events::TenantEventPublisher,
    files::upload_file::{UploadFile, upload_file},
};
use docbox_processing::ProcessingLayerConfig;

mod common;

/// Tests that a simple text file upload succeeds
#[tokio::test]
async fn test_file_create_success() {
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

    upload_file(
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
}
