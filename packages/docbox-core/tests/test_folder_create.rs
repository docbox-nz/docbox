use crate::common::{database::create_test_tenant_database, search::create_test_tenant_typesense};
use docbox_core::{
    document_box::create_document_box::{CreateDocumentBox, create_document_box},
    events::{TenantEventMessage, TenantEventPublisher, mpsc::MpscEventPublisher},
    folders::create_folder::{CreateFolderData, safe_create_folder},
};
use docbox_search::models::{SearchIndexType, SearchRequest};

mod common;

/// Tests that a folder can be created successfully
#[tokio::test]
async fn test_create_folder_success() {
    let (_container_db, db) = create_test_tenant_database().await;
    let (_container_search, search) = create_test_tenant_typesense().await;
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

    // Ensure the correct data was inserted
    assert_eq!(folder.name, "Test Folder");
    assert_eq!(folder.created_by, None);

    // Expect creation event
    let event = events_rx.recv().await.unwrap();
    assert!(matches!(
        event,
        TenantEventMessage::FolderCreated(created) if created.data.id == folder.id
    ));

    // Ensure the name is correctly indexed and searchable
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

        assert_eq!(result.total_hits, 1);
        assert_eq!(result.results.len(), 1);
        let first = result.results.first().unwrap();

        assert_eq!(first.item_id, folder.id);
        assert!(
            matches!(first.item_ty, SearchIndexType::Folder),
            "expecting folder search index type"
        );
        assert_eq!(first.document_box, document_box.scope);
        assert!(first.page_matches.is_empty());
        assert_eq!(first.total_hits, 1);
        assert!(first.name_match);
        assert!(!first.content_match);
    }
}
