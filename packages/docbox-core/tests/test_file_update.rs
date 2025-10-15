use crate::common::{
    database::create_test_tenant_database, processing::create_processing_layer,
    search::create_test_tenant_typesense, storage::create_test_tenant_storage,
};
use docbox_core::{
    document_box::create_document_box::{CreateDocumentBox, create_document_box},
    events::TenantEventPublisher,
    files::{
        update_file::{UpdateFile, update_file},
        upload_file::{UploadFile, safe_upload_file},
    },
};
use docbox_database::models::file::File;
use docbox_search::models::{SearchIndexType, SearchRequest};

mod common;

/// Tests that a file can be deleted
#[tokio::test]
async fn test_update_file_name_success() {
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

    update_file(
        &db,
        &search,
        &document_box.scope,
        file.clone(),
        None,
        UpdateFile {
            folder_id: None,
            name: Some("Other Name Which Should Never Match.txt".to_string()),
            pinned: None,
        },
    )
    .await
    .unwrap();

    // Ensure the file name is updated in the database
    {
        let updated_file = File::find(&db, &document_box.scope, file.id)
            .await
            .unwrap()
            .expect("missing uploaded file");

        assert_eq!(
            updated_file.name.as_str(),
            "Other Name Which Should Never Match.txt"
        );
    }

    // Ensure the name is correctly removed from the index and is not searchable
    {
        let request = SearchRequest {
            query: Some("test.txt".to_string()),
            include_name: true,
            ..Default::default()
        };

        let result = search
            .search_index(&[document_box.scope.to_string()], request, None)
            .await
            .unwrap();

        assert_eq!(result.total_hits, 0);
        assert!(result.results.is_empty());
    }

    // Ensure the new name is correctly indexed and searchable
    {
        let request = SearchRequest {
            query: Some("Other Name Which Should Never Match.txt".to_string()),
            include_name: true,
            ..Default::default()
        };

        let result = search
            .search_index(&[document_box.scope.to_string()], request, None)
            .await
            .unwrap();

        assert_eq!(result.total_hits, 1);
        assert_eq!(result.results.len(), 1);
        let first = result.results.first().unwrap();

        assert_eq!(first.item_id, file.id);
        assert!(
            matches!(first.item_ty, SearchIndexType::File),
            "expecting file search index type"
        );
        assert_eq!(first.document_box, document_box.scope);
        assert!(first.page_matches.is_empty());
        assert_eq!(first.total_hits, 1);
        assert!(first.name_match);
        assert!(!first.content_match);
    }
}
