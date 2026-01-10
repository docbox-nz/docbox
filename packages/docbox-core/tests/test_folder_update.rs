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

use crate::common::{database::test_tenant_db, tenant::test_tenant, typesense::test_tenant_search};

mod common;

/// Tests that a folder name can be updated successfully
#[tokio::test]
async fn test_update_folder_name_success() {
    let tenant = test_tenant();

    let (db, _db_container) = test_tenant_db().await;
    let (search, _search_container) = test_tenant_search(&tenant).await;

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

    // Ensure the folder name is updated in the database
    {
        let updated_folder = Folder::find_by_id(&db, &"test".to_string(), folder.id)
            .await
            .unwrap()
            .expect("missing updated folder");

        assert_eq!(
            updated_folder.name.as_str(),
            "Other Name Which Should Never Match"
        );
    }

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
    let tenant = test_tenant();

    let (db, _db_container) = test_tenant_db().await;
    let (search, _search_container) = test_tenant_search(&tenant).await;

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

    // Ensure the folder parent is updated in the database
    {
        let updated_folder = Folder::find_by_id(&db, &"test".to_string(), folder.id)
            .await
            .unwrap()
            .expect("missing updated folder");

        assert_eq!(updated_folder.folder_id, Some(new_folder.id));
    }

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
    let tenant = test_tenant();

    let (db, _db_container) = test_tenant_db().await;
    let (search, _search_container) = test_tenant_search(&tenant).await;

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
    let tenant = test_tenant();

    let (db, _db_container) = test_tenant_db().await;
    let (search, _search_container) = test_tenant_search(&tenant).await;

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
    let tenant = test_tenant();

    let (db, _db_container) = test_tenant_db().await;
    let (search, _search_container) = test_tenant_search(&tenant).await;

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

    // Create a folder within the folder we are trying to move
    let child_folder = safe_create_folder(
        &db,
        search.clone(),
        &events,
        CreateFolderData {
            folder: folder.clone(),
            name: "Test Child Folder".to_string(),
            created_by: None,
        },
    )
    .await
    .unwrap();

    // Try and move the folder into its child folder
    let err = update_folder(
        &db,
        &search,
        &"test".to_string(),
        folder.clone(),
        None,
        UpdateFolder {
            folder_id: Some(child_folder.id),
            name: None,
            pinned: None,
        },
    )
    .await
    .unwrap_err();

    assert!(
        matches!(err, UpdateFolderError::CannotMoveIntoChildOfSelf),
        "moving to child of self should result in a failure"
    );
}

/// Tests that a root folder cannot be updated
#[tokio::test]
async fn test_update_folder_folder_root() {
    let tenant = test_tenant();

    let (db, _db_container) = test_tenant_db().await;
    let (search, _search_container) = test_tenant_search(&tenant).await;

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
