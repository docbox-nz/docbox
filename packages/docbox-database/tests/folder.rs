use docbox_database::models::{
    document_box::DocumentBox,
    folder::{CreateFolder, Folder},
    link::{CreateLink, Link},
    shared::{DocboxInputPair, FolderPathSegment},
};

use crate::common::{database::test_tenant_db, make_test_document_box, make_test_folder};

mod common;

#[tokio::test]
async fn test_tree_all_folder_children() {
    let (db, _db_container) = test_tenant_db().await;

    let scope = "test".to_string();
    _ = DocumentBox::create(&db, scope.clone()).await.unwrap();

    let root = Folder::create(
        &db,
        CreateFolder {
            name: "Root".to_string(),
            document_box: scope.to_string(),
            folder_id: None,
            created_by: None,
        },
    )
    .await
    .unwrap();

    let base_folder = Folder::create(
        &db,
        CreateFolder {
            name: "base".to_string(),
            document_box: scope.clone(),
            folder_id: Some(root.id),
            created_by: None,
        },
    )
    .await
    .unwrap();

    let nested_folder = Folder::create(
        &db,
        CreateFolder {
            name: "nested".to_string(),
            document_box: scope.clone(),
            folder_id: Some(base_folder.id),
            created_by: None,
        },
    )
    .await
    .unwrap();

    let nested_path = root.tree_all_children(&db).await.unwrap();

    assert_eq!(nested_path.len(), 3);
    assert_eq!(nested_path, vec![root.id, base_folder.id, nested_folder.id]);
}

#[tokio::test]
async fn test_count_folder_children() {
    let (db, _db_container) = test_tenant_db().await;

    let scope = "test".to_string();
    _ = DocumentBox::create(&db, scope.clone()).await.unwrap();

    let root = Folder::create(
        &db,
        CreateFolder {
            name: "Root".to_string(),
            document_box: scope.to_string(),
            folder_id: None,
            created_by: None,
        },
    )
    .await
    .unwrap();

    let base_folder = Folder::create(
        &db,
        CreateFolder {
            name: "base".to_string(),
            document_box: scope.clone(),
            folder_id: Some(root.id),
            created_by: None,
        },
    )
    .await
    .unwrap();

    let _nested_folder = Folder::create(
        &db,
        CreateFolder {
            name: "nested".to_string(),
            document_box: scope.clone(),
            folder_id: Some(base_folder.id),
            created_by: None,
        },
    )
    .await
    .unwrap();

    Link::create(
        &db,
        CreateLink {
            name: "Test".to_string(),
            value: "http://test.com".to_string(),
            folder_id: base_folder.id,
            created_by: None,
        },
    )
    .await
    .unwrap();

    let counts = Folder::count_children(&db, root.id).await.unwrap();

    assert_eq!(counts.folder_count, 2);
    assert_eq!(counts.link_count, 1);
}

#[tokio::test]
async fn test_folder_resolve_path() {
    let (db, _db_container) = test_tenant_db().await;

    let scope = "test".to_string();
    _ = DocumentBox::create(&db, scope.clone()).await.unwrap();

    let root = Folder::create(
        &db,
        CreateFolder {
            name: "Root".to_string(),
            document_box: scope.to_string(),
            folder_id: None,
            created_by: None,
        },
    )
    .await
    .unwrap();

    let base_folder = Folder::create(
        &db,
        CreateFolder {
            name: "base".to_string(),
            document_box: scope.clone(),
            folder_id: Some(root.id),
            created_by: None,
        },
    )
    .await
    .unwrap();

    let nested_folder = Folder::create(
        &db,
        CreateFolder {
            name: "nested".to_string(),
            document_box: scope.clone(),
            folder_id: Some(base_folder.id),
            created_by: None,
        },
    )
    .await
    .unwrap();

    let nested_path = Folder::resolve_path(&db, nested_folder.id).await.unwrap();

    assert_eq!(nested_path.len(), 2);

    assert_eq!(nested_path[0].id, root.id);
    assert_eq!(nested_path[0].name, root.name);

    assert_eq!(nested_path[1].id, base_folder.id);
    assert_eq!(nested_path[1].name, base_folder.name);
}

#[tokio::test]
async fn test_move_to_folder_folder() {}

#[tokio::test]
async fn test_rename_folder() {}

#[tokio::test]
async fn test_set_pinned_folder() {}

#[tokio::test]
async fn test_find_by_id_folder() {}

#[tokio::test]
async fn test_all_non_root_folder() {}

#[tokio::test]
async fn test_find_by_parent_folder() {}

/// Tests that the root folder can be found for a document box
#[tokio::test]
async fn test_find_root_folder() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, root) = make_test_document_box(&db, "test_1", None).await;

    // Should be able to find valid roots
    let found_root = Folder::find_root(&db, &document_box.scope)
        .await
        .unwrap()
        .expect("folder should exist");
    assert_eq!(found_root, root);

    // Shouldn't be able to find a non existent root
    let invalid_root = Folder::find_root(&db, &"test_3".to_string()).await.unwrap();
    assert!(invalid_root.is_none());
}

/// Tests that folders can be created
#[tokio::test]
async fn test_create_folder() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, root) = make_test_document_box(&db, "test_1", None).await;

    assert_eq!(root.name, "Root");
    assert_eq!(root.document_box, document_box.scope);
    assert!(root.folder_id.is_none());

    let base_folder = Folder::create(
        &db,
        CreateFolder {
            name: "base".to_string(),
            document_box: document_box.scope.clone(),
            folder_id: Some(root.id),
            created_by: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(base_folder.name, "base");
    assert_eq!(base_folder.document_box, document_box.scope);
    assert_eq!(base_folder.folder_id, Some(root.id));

    let root_result = Folder::find_by_id(&db, &document_box.scope, root.id)
        .await
        .unwrap()
        .expect("folder should exist");

    assert_eq!(root_result, root);

    let base_result = Folder::find_by_id(&db, &document_box.scope, base_folder.id)
        .await
        .unwrap()
        .expect("folder should exist");

    assert_eq!(base_result, base_folder);
}

/// Tests that a folder can be deleted successfully and ensures no other rows are
/// affected and constraints are enforced
#[tokio::test]
async fn test_delete_folder() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, root) = make_test_document_box(&db, "test_1", None).await;

    let folder = make_test_folder(&db, &root, "test", None).await;
    let other_folder = make_test_folder(&db, &root, "test_2", None).await;

    // Folder should exist
    let target = Folder::find_by_id(&db, &document_box.scope, folder.id)
        .await
        .unwrap();
    assert!(target.is_some());

    // Delete folder should delete one row
    let result = folder.delete(&db).await.unwrap();
    assert_eq!(result.rows_affected(), 1);

    // Folder shouldn't exist
    let target = Folder::find_by_id(&db, &document_box.scope, folder.id)
        .await
        .unwrap();
    assert!(target.is_none());

    // Other folder should still exist
    let target = Folder::find_by_id(&db, &document_box.scope, other_folder.id)
        .await
        .unwrap();
    assert!(target.is_some());

    // Delete folder shouldn't delete any rows now that its gone
    let result = folder.delete(&db).await.unwrap();
    assert_eq!(result.rows_affected(), 0);

    // Should not be able to delete the root while another folder is still present
    // (Enforce proper deletion)
    let result = root.delete(&db).await.unwrap_err();
    assert_eq!(
        result.into_database_error().unwrap().code().unwrap(),
        // RESTRICT foreign key constraint violation
        "23001"
    );
}

#[tokio::test]
async fn test_resolve_folder_with_extra_mixed_scopes() {
    let (db, _db_container) = test_tenant_db().await;

    let (_scope_1, root) = make_test_document_box(&db, "test_1", None).await;
    let (_scope_2, root_2) = make_test_document_box(&db, "test_2", None).await;

    let base_folder = make_test_folder(&db, &root, "base", None).await;
    let base_folder_2 = make_test_folder(&db, &root_2, "base_2", None).await;
    let nested_folder = make_test_folder(&db, &base_folder, "nested", None).await;
    let nested_folder_2 = make_test_folder(&db, &base_folder, "nested_2", None).await;
    let nested_folder_3 = make_test_folder(&db, &base_folder_2, "nested_23", None).await;

    let resolved = Folder::resolve_with_extra_mixed_scopes(
        &db,
        vec![
            DocboxInputPair::new(&nested_folder.document_box, nested_folder.id),
            DocboxInputPair::new(&nested_folder_2.document_box, nested_folder_2.id),
            DocboxInputPair::new(&base_folder_2.document_box, base_folder_2.id),
            DocboxInputPair::new(&nested_folder_3.document_box, nested_folder_3.id),
        ],
    )
    .await
    .unwrap();

    assert_eq!(resolved.len(), 4);

    let resolved_1 = resolved
        .iter()
        .find(|item| item.data.folder.id == nested_folder.id)
        .unwrap();

    assert_eq!(resolved_1.data.folder, nested_folder);
    assert_eq!(resolved_1.data.created_by, None);
    assert_eq!(resolved_1.data.last_modified_by, None);
    assert_eq!(resolved_1.data.last_modified_at, None);
    assert_eq!(
        &resolved_1.full_path,
        &[
            FolderPathSegment::from(&root),
            FolderPathSegment::from(&base_folder)
        ]
    );

    let resolved_2 = resolved
        .iter()
        .find(|item| item.data.folder.id == nested_folder_2.id)
        .unwrap();

    assert_eq!(resolved_2.data.folder, nested_folder_2);
    assert_eq!(resolved_2.data.created_by, None);
    assert_eq!(resolved_2.data.last_modified_by, None);
    assert_eq!(resolved_2.data.last_modified_at, None);
    assert_eq!(
        &resolved_2.full_path,
        &[
            FolderPathSegment::from(&root),
            FolderPathSegment::from(&base_folder)
        ]
    );

    let resolved_3 = resolved
        .iter()
        .find(|item| item.data.folder.id == base_folder_2.id)
        .unwrap();

    assert_eq!(resolved_3.data.folder, base_folder_2);
    assert_eq!(resolved_3.data.created_by, None);
    assert_eq!(resolved_3.data.last_modified_by, None);
    assert_eq!(resolved_3.data.last_modified_at, None);
    assert_eq!(&resolved_3.full_path, &[FolderPathSegment::from(&root_2)]);

    let resolved_4 = resolved
        .iter()
        .find(|item| item.data.folder.id == nested_folder_3.id)
        .unwrap();

    assert_eq!(resolved_4.data.folder, nested_folder_3);
    assert_eq!(resolved_4.data.created_by, None);
    assert_eq!(resolved_4.data.last_modified_by, None);
    assert_eq!(resolved_4.data.last_modified_at, None);
    assert_eq!(
        &resolved_4.full_path,
        &[
            FolderPathSegment::from(&root_2),
            FolderPathSegment::from(&base_folder_2)
        ]
    );
}

#[tokio::test]
async fn test_resolve_with_extra_folder() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, root) = make_test_document_box(&db, "test", None).await;
    let base_folder = make_test_folder(&db, &root, "base", None).await;
    let base_folder_2 = make_test_folder(&db, &root, "base_2", None).await;
    let nested_folder = make_test_folder(&db, &base_folder, "nested", None).await;
    let nested_folder_2 = make_test_folder(&db, &base_folder, "nested_2", None).await;
    let nested_folder_3 = make_test_folder(&db, &base_folder_2, "nested_3", None).await;

    let resolved = Folder::resolve_with_extra(
        &db,
        &document_box.scope,
        vec![
            nested_folder.id,
            nested_folder_2.id,
            base_folder_2.id,
            nested_folder_3.id,
        ],
    )
    .await
    .unwrap();

    assert_eq!(resolved.len(), 4);

    let resolved_1 = resolved
        .iter()
        .find(|item| item.data.folder.id == nested_folder.id)
        .unwrap();

    assert_eq!(
        &resolved_1.full_path,
        &[
            FolderPathSegment::from(&root),
            FolderPathSegment::from(&base_folder)
        ]
    );

    let resolved_2 = resolved
        .iter()
        .find(|item| item.data.folder.id == nested_folder_2.id)
        .unwrap();

    assert_eq!(
        &resolved_2.full_path,
        &[
            FolderPathSegment::from(&root),
            FolderPathSegment::from(&base_folder)
        ]
    );

    let resolved_3 = resolved
        .iter()
        .find(|item| item.data.folder.id == base_folder_2.id)
        .unwrap();

    assert_eq!(&resolved_3.full_path, &[FolderPathSegment::from(&root)]);

    let resolved_4 = resolved
        .iter()
        .find(|item| item.data.folder.id == nested_folder_3.id)
        .unwrap();

    assert_eq!(
        &resolved_4.full_path,
        &[
            FolderPathSegment::from(&root),
            FolderPathSegment::from(&base_folder_2)
        ]
    );
}

#[tokio::test]
async fn test_resolve_by_id_with_extra_folder() {}

#[tokio::test]
async fn test_find_by_parent_with_extra_folder() {}

#[tokio::test]
async fn test_find_root_with_extra_folder() {}

#[tokio::test]
async fn test_total_count_folder() {}

#[tokio::test]
async fn test_resolved_folder() {}

#[tokio::test]
async fn test_resolved_folder_with_extra() {}
