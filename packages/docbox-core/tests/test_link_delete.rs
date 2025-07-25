use crate::common::{database::create_test_tenant_database, search::create_test_tenant_typesense};
use docbox_core::{
    document_box::create_document_box::{CreateDocumentBox, create_document_box},
    events::{TenantEventMessage, TenantEventPublisher, mpsc::MpscEventPublisher},
    links::{
        create_link::{CreateLinkData, safe_create_link},
        delete_link::delete_link,
    },
};
use docbox_database::models::link::Link;
use docbox_search::models::SearchRequest;
use uuid::Uuid;

mod common;

/// Tests that a link can be deleted successfully
#[tokio::test]
async fn test_delete_link_success() {
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

    // Consume creation event
    _ = events_rx.recv().await.unwrap();

    // Ensure the correct data was inserted
    assert_eq!(link.name, "Test Link");
    assert_eq!(link.value, "http://example.com");
    assert_eq!(link.created_by, None);

    let link_id = link.id;

    // Delete the link
    delete_link(&db, &search, &events, link, document_box.scope.to_string())
        .await
        .unwrap();

    // Expect deletion event
    let event = events_rx.recv().await.unwrap();
    assert!(matches!(
        event,
        TenantEventMessage::LinkDeleted(deleted) if deleted.data.id == link_id
    ));

    // Ensure the link cannot be found
    {
        let has_link = Link::find(&db, &document_box.scope, link_id)
            .await
            .unwrap()
            .is_some();
        assert!(!has_link);
    }

    // Ensure the name is correctly removed from the index and is not searchable
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

        assert_eq!(result.total_hits, 0);
        assert!(result.results.is_empty());
    }

    // Ensure the value is correctly removed from the index and is not searchable
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

        assert_eq!(result.total_hits, 0);
        assert!(result.results.is_empty());
    }
}

/// Tests that attempt to delete a non-existent link should not
/// produce any events
#[tokio::test]
async fn test_delete_unknown_link() {
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

    let fake_link = Link {
        id: Uuid::new_v4(),
        name: Default::default(),
        value: Default::default(),
        folder_id: root.id,
        created_at: Default::default(),
        created_by: Default::default(),
        pinned: Default::default(),
    };

    // Delete the link
    delete_link(
        &db,
        &search,
        &events,
        fake_link,
        document_box.scope.to_string(),
    )
    .await
    .unwrap();

    // Should have nothing to consume
    assert!(events_rx.try_recv().is_err());
}
