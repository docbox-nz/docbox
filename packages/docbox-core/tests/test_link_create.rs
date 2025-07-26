use docbox_core::{
    document_box::create_document_box::{CreateDocumentBox, create_document_box},
    events::{TenantEventMessage, TenantEventPublisher, mpsc::MpscEventPublisher},
    links::create_link::{CreateLinkData, safe_create_link},
};
use docbox_search::models::{SearchIndexType, SearchRequest};

use crate::common::{database::create_test_tenant_database, search::create_test_tenant_typesense};

mod common;

/// Tests that a link can be created successfully
#[tokio::test]
async fn test_create_link_success() {
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

    let link = safe_create_link(
        &db,
        search.clone(),
        &events,
        CreateLinkData {
            folder: root,
            name: "Test Link".to_string(),
            value: "http://example.com".to_string(),
            created_by: None,
        },
    )
    .await
    .unwrap();

    // Ensure the correct data was inserted
    assert_eq!(link.name, "Test Link");
    assert_eq!(link.value, "http://example.com");
    assert_eq!(link.created_by, None);

    // Expect creation event
    let event = events_rx.recv().await.unwrap();
    assert!(matches!(
        event,
        TenantEventMessage::LinkCreated(created) if created.data.id == link.id
    ));

    // Ensure the name is correctly indexed and searchable
    {
        let request = SearchRequest {
            query: Some("Test Link".to_string()),
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

        assert_eq!(first.item_id, link.id);
        assert!(
            matches!(first.item_ty, SearchIndexType::Link),
            "expecting link search index type"
        );
        assert_eq!(first.document_box, document_box.scope);
        assert!(first.page_matches.is_empty());
        assert_eq!(first.total_hits, 1);
        assert!(first.name_match);
        assert!(!first.content_match);
    }

    // Ensure the value is correctly indexed and searchable
    {
        let request = SearchRequest {
            query: Some("http://example.com".to_string()),
            include_content: true,
            ..Default::default()
        };

        let result = search
            .search_index(&["test".to_string()], request, None)
            .await
            .unwrap();

        assert_eq!(result.total_hits, 1);
        assert_eq!(result.results.len(), 1);
        let first = result.results.first().unwrap();

        assert_eq!(first.item_id, link.id);
        assert!(
            matches!(first.item_ty, SearchIndexType::Link),
            "expecting link search index type"
        );
        assert_eq!(first.document_box, document_box.scope);
        assert!(first.page_matches.is_empty());
        assert_eq!(first.total_hits, 1);
        assert!(!first.name_match);
        assert!(first.content_match);
    }
}
