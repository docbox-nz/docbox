use docbox_core::{
    document_box::create_document_box::{CreateDocumentBox, create_document_box},
    events::{TenantEventPublisher, noop::NoopEventPublisher},
    folders::{
        create_folder::{CreateFolderData, safe_create_folder},
        update_folder::{UpdateFolder, UpdateFolderError, update_folder},
    },
};
use docbox_database::models::folder::Folder;
use docbox_search::models::{SearchIndexType, SearchRequest};
use uuid::Uuid;

use crate::common::{database::create_test_tenant_database, search::create_test_tenant_typesense};

mod common;

/// Tests that a folder name can be updated successfully
#[tokio::test]
async fn test_update_folder_name_success() {
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

    // Update the folder
    update_folder(
        &db,
        &search,
        &"test".to_string(),
        folder.clone(),
        None,
        UpdateFolder {
            folder_id: None,
            name: Some("Other Name Which Should Never Match".to_string()),
            pinned: None,
        },
    )
    .await
    .unwrap();

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

/// Tests that a folder can be moved to another folder
#[tokio::test]
async fn test_update_folder_folder_success() {
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

    let folder = safe_create_folder(
        &db,
        search.clone(),
        &events,
        CreateFolderData {
            folder: test_folder.clone(),
            name: "Test Folder".to_string(),
            created_by: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(folder.folder_id.unwrap(), test_folder.id);

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

    // Update the folder
    update_folder(
        &db,
        &search,
        &"test".to_string(),
        folder.clone(),
        None,
        UpdateFolder {
            folder_id: Some(new_folder.id),
            name: None,
            pinned: None,
        },
    )
    .await
    .unwrap();

    // Ensure the folder is no longer apart of the old folder
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

    // Ensure the folder is apart of the new folder
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

        assert_eq!(first.item_id, folder.id);
        assert!(
            matches!(first.item_ty, SearchIndexType::Folder),
            "expecting folder search index type"
        );
        assert_eq!(first.document_box, document_box.scope);
        assert!(first.page_matches.is_empty());
        assert_eq!(first.total_hits, 1);
    }
}

/// Tests that a folder pinned state can be updated
#[tokio::test]
async fn test_update_folder_pinned_success() {
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

    let folder = safe_create_folder(
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

    // Update the folder
    update_folder(
        &db,
        &search,
        &document_box.scope,
        folder.clone(),
        None,
        UpdateFolder {
            folder_id: None,
            name: None,
            pinned: Some(true),
        },
    )
    .await
    .unwrap();

    let folder = Folder::find_by_id(&db, &document_box.scope, folder.id)
        .await
        .unwrap()
        .unwrap();

    assert!(folder.pinned);

    // Update the folder
    update_folder(
        &db,
        &search,
        &document_box.scope,
        folder.clone(),
        None,
        UpdateFolder {
            folder_id: None,
            name: None,
            pinned: Some(false),
        },
    )
    .await
    .unwrap();

    let folder = Folder::find_by_id(&db, &document_box.scope, folder.id)
        .await
        .unwrap()
        .unwrap();

    assert!(!folder.pinned);
}

/// Tests that a folder cannot be moved to an unknown folder
#[tokio::test]
async fn test_update_folder_folder_unknown() {
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

    let folder = safe_create_folder(
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

    assert_eq!(folder.folder_id.unwrap(), root.id);

    // Update the folder
    let err = update_folder(
        &db,
        &search,
        &"test".to_string(),
        folder.clone(),
        None,
        UpdateFolder {
            folder_id: Some(Uuid::nil()),
            name: None,
            pinned: None,
        },
    )
    .await
    .unwrap_err();

    assert!(
        matches!(err, UpdateFolderError::UnknownTargetFolder),
        "unknown folder should result in a failure"
    );
}

/// Tests that a folder cannot be moved into itself
#[tokio::test]
async fn test_update_folder_folder_self() {
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

    let folder = safe_create_folder(
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

    assert_eq!(folder.folder_id.unwrap(), root.id);

    // Update the folder
    let err = update_folder(
        &db,
        &search,
        &"test".to_string(),
        folder.clone(),
        None,
        UpdateFolder {
            folder_id: Some(folder.id),
            name: None,
            pinned: None,
        },
    )
    .await
    .unwrap_err();

    assert!(
        matches!(err, UpdateFolderError::CannotMoveIntoSelf),
        "moving to self should result in a failure"
    );
}

/// Tests that a root folder cannot be updated
#[tokio::test]
async fn test_update_folder_folder_root() {
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

    // Update the folder
    let err = update_folder(
        &db,
        &search,
        &"test".to_string(),
        root,
        None,
        UpdateFolder {
            folder_id: None,
            name: None,
            pinned: None,
        },
    )
    .await
    .unwrap_err();

    assert!(
        matches!(err, UpdateFolderError::CannotModifyRoot),
        "modifying root should result in a failure"
    );
}
