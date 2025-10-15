use docbox_core::{
    document_box::create_document_box::{CreateDocumentBox, create_document_box},
    events::{TenantEventPublisher, noop::NoopEventPublisher},
    folders::create_folder::{CreateFolderData, safe_create_folder},
    links::{
        create_link::{CreateLinkData, safe_create_link},
        update_link::{UpdateLink, UpdateLinkError, update_link},
    },
};
use docbox_database::models::link::Link;
use docbox_search::models::{SearchIndexType, SearchRequest};
use uuid::Uuid;

use crate::common::{database::create_test_tenant_database, search::create_test_tenant_typesense};

mod common;

/// Tests that a link name can be updated successfully
#[tokio::test]
async fn test_update_link_name_success() {
    let (_container_db, db) = create_test_tenant_database().await;
    let (_container_search, search) = create_test_tenant_typesense().await;
    let events = TenantEventPublisher::Noop(NoopEventPublisher);
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

    // Update the link
    update_link(
        &db,
        &search,
        &"test".to_string(),
        link.clone(),
        None,
        UpdateLink {
            folder_id: None,
            name: Some("Other Name Which Should Never Match".to_string()),
            value: None,
            pinned: None,
        },
    )
    .await
    .unwrap();

    // Ensure the link name is updated in the database
    {
        let updated_link = Link::find(&db, &"test".to_string(), link.id)
            .await
            .unwrap()
            .expect("missing updated link");

        assert_eq!(
            updated_link.name.as_str(),
            "Other Name Which Should Never Match"
        );
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

    // Ensure the new name is correctly indexed and searchable
    {
        let request = SearchRequest {
            query: Some("Other Name Which Should Never Match".to_string()),
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
}

/// Tests that a link value can be updated successfully
#[tokio::test]
async fn test_update_link_value_success() {
    let (_container_db, db) = create_test_tenant_database().await;
    let (_container_search, search) = create_test_tenant_typesense().await;
    let events = TenantEventPublisher::Noop(NoopEventPublisher);
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

    // Update the link
    update_link(
        &db,
        &search,
        &"test".to_string(),
        link.clone(),
        None,
        UpdateLink {
            folder_id: None,
            name: None,
            value: Some("http://test.com".to_string()),
            pinned: None,
        },
    )
    .await
    .unwrap();

    // Ensure the link value is updated in the database
    {
        let updated_link = Link::find(&db, &"test".to_string(), link.id)
            .await
            .unwrap()
            .expect("missing updated link");

        assert_eq!(updated_link.value.as_str(), "http://test.com");
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

    // Ensure the value is correctly indexed and searchable
    {
        let request = SearchRequest {
            query: Some("http://test.com".to_string()),
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

/// Tests that a link pinned state can be updated successfully
#[tokio::test]
async fn test_update_link_pinned_success() {
    let (_container_db, db) = create_test_tenant_database().await;
    let (_container_search, search) = create_test_tenant_typesense().await;
    let events = TenantEventPublisher::Noop(NoopEventPublisher);
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

    // Update the link
    update_link(
        &db,
        &search,
        &document_box.scope,
        link.clone(),
        None,
        UpdateLink {
            folder_id: None,
            name: None,
            value: None,
            pinned: Some(true),
        },
    )
    .await
    .unwrap();

    let link = Link::find(&db, &document_box.scope, link.id)
        .await
        .unwrap()
        .unwrap();

    assert!(link.pinned);

    // Update the link
    update_link(
        &db,
        &search,
        &document_box.scope,
        link.clone(),
        None,
        UpdateLink {
            folder_id: None,
            name: None,
            value: None,
            pinned: Some(false),
        },
    )
    .await
    .unwrap();

    let link = Link::find(&db, &document_box.scope, link.id)
        .await
        .unwrap()
        .unwrap();

    assert!(!link.pinned);
}

/// Tests that a link can be moved to another folder
#[tokio::test]
async fn test_update_link_folder_success() {
    let (_container_db, db) = create_test_tenant_database().await;
    let (_container_search, search) = create_test_tenant_typesense().await;
    let events = TenantEventPublisher::Noop(NoopEventPublisher);
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

    let test_folder = safe_create_folder(
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

    let link = safe_create_link(
        &db,
        search.clone(),
        &events,
        CreateLinkData {
            folder: test_folder.clone(),
            name: "Test Link".to_string(),
            value: "http://example.com".to_string(),
            created_by: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(link.folder_id, test_folder.id);

    let new_folder = safe_create_folder(
        &db,
        search.clone(),
        &events,
        CreateFolderData {
            folder: root.clone(),
            name: "New Folder".to_string(),
            created_by: None,
        },
    )
    .await
    .unwrap();

    // Update the link
    update_link(
        &db,
        &search,
        &"test".to_string(),
        link.clone(),
        None,
        UpdateLink {
            folder_id: Some(new_folder.id),
            name: None,
            value: None,
            pinned: None,
        },
    )
    .await
    .unwrap();

    // Ensure the link folder id is updated in the database
    {
        let updated_link = Link::find(&db, &"test".to_string(), link.id)
            .await
            .unwrap()
            .expect("missing updated link");

        assert_eq!(updated_link.folder_id, new_folder.id);
    }

    // Ensure the link is no longer apart of the old folder
    {
        let request = SearchRequest {
            folder_id: Some(test_folder.id),
            ..Default::default()
        };

        let result = search
            .search_index(&["test".to_string()], request, None)
            .await
            .unwrap();

        assert_eq!(result.total_hits, 0);
        assert!(result.results.is_empty());
    }

    // Ensure the link is apart of the new folder
    {
        let request = SearchRequest {
            folder_id: Some(new_folder.id),
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
    }
}

/// Tests that a link cannot be moved to an unknown folder
#[tokio::test]
async fn test_update_link_folder_unknown() {
    let (_container_db, db) = create_test_tenant_database().await;
    let (_container_search, search) = create_test_tenant_typesense().await;
    let events = TenantEventPublisher::Noop(NoopEventPublisher);
    let (_document_box, root) = create_document_box(
        &db,
        &events,
        CreateDocumentBox {
            scope: "test".to_string(),
            created_by: None,
        },
    )
    .await
    .unwrap();

    let link = safe_create_link(
        &db,
        search.clone(),
        &events,
        CreateLinkData {
            folder: root.clone(),
            name: "Test Link".to_string(),
            value: "http://example.com".to_string(),
            created_by: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(link.folder_id, root.id);

    // Update the link
    let err = update_link(
        &db,
        &search,
        &"test".to_string(),
        link.clone(),
        None,
        UpdateLink {
            folder_id: Some(Uuid::nil()),
            name: None,
            value: None,
            pinned: None,
        },
    )
    .await
    .unwrap_err();

    assert!(
        matches!(err, UpdateLinkError::UnknownTargetFolder),
        "unknown folder should result in a failure"
    );
}
