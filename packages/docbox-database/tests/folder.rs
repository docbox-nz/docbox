use crate::common::{
    database::test_tenant_db, make_test_document_box, make_test_folder, make_test_link,
    make_test_user,
};
use chrono::Utc;
use docbox_database::{
    models::{
        file::{CreateFile, File},
        folder::{CreateFolder, Folder, ResolvedFolder, ResolvedFolderWithExtra},
        link::{CreateLink, Link},
        shared::{DocboxInputPair, FolderPathSegment},
    },
    utils::DatabaseErrorExt,
};
use uuid::Uuid;

mod common;

/// Tests that folders can be created and subsequently retrieved
#[tokio::test]
async fn test_folder_create() {
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

/// Tests that all the child folders of a created folder can be listed
#[tokio::test]
async fn test_folder_tree_all_children() {
    let (db, _db_container) = test_tenant_db().await;
    let (_document_box, root) = make_test_document_box(&db, "test", None).await;

    let nested_path = root.tree_all_children(&db).await.unwrap();
    assert_eq!(&nested_path, &[root.id]);

    let base_folder = make_test_folder(&db, &root, "base", None).await;

    let nested_path = root.tree_all_children(&db).await.unwrap();
    assert_eq!(&nested_path, &[root.id, base_folder.id]);

    let nested_folder = make_test_folder(&db, &base_folder, "nested", None).await;

    let nested_path = root.tree_all_children(&db).await.unwrap();
    assert_eq!(&nested_path, &[root.id, base_folder.id, nested_folder.id]);
}

/// Tests that the children of a folder can be counted
#[tokio::test]
async fn test_folder_count_children() {
    let (db, _db_container) = test_tenant_db().await;
    let (_document_box, root) = make_test_document_box(&db, "test", None).await;
    let base_folder = make_test_folder(&db, &root, "base", None).await;

    // Should be empty initially
    let counts = Folder::count_children(&db, base_folder.id).await.unwrap();
    assert_eq!(counts.folder_count, 0);
    assert_eq!(counts.link_count, 0);
    assert_eq!(counts.file_count, 0);

    const FOLDER_COUNT: i64 = 10;
    const LINK_COUNT: i64 = 15;
    const FILE_COUNT: i64 = 12;

    for i in 0..FOLDER_COUNT {
        let _nested_folder = make_test_folder(&db, &base_folder, format!("nested_{i}"), None).await;
    }

    for i in 0..LINK_COUNT {
        Link::create(
            &db,
            CreateLink {
                name: format!("Test {i}"),
                value: "http://test.com".to_string(),
                folder_id: base_folder.id,
                created_by: None,
            },
        )
        .await
        .unwrap();
    }

    for i in 0..FILE_COUNT {
        File::create(
            &db,
            CreateFile {
                id: Uuid::new_v4(),
                name: format!("File {i}"),
                folder_id: base_folder.id,
                mime: "text/plain".to_string(),
                file_key: "test".to_string(),
                created_at: Utc::now(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    }

    let counts = Folder::count_children(&db, base_folder.id).await.unwrap();
    assert_eq!(counts.folder_count, FOLDER_COUNT);
    assert_eq!(counts.link_count, LINK_COUNT);
    assert_eq!(counts.file_count, FILE_COUNT);
}

/// Tests that the path of a folder can be resolved
#[tokio::test]
async fn test_folder_resolve_path() {
    let (db, _db_container) = test_tenant_db().await;
    let (_document_box, root) = make_test_document_box(&db, "test", None).await;

    // Path to root should be empty
    let root_path = Folder::resolve_path(&db, root.id).await.unwrap();
    assert_eq!(&root_path, &[]);

    let base_folder = make_test_folder(&db, &root, "base", None).await;

    // Path to base folder should contain root
    let base_path = Folder::resolve_path(&db, base_folder.id).await.unwrap();
    assert_eq!(&base_path, &[FolderPathSegment::from(&root),]);

    let nested_folder = make_test_folder(&db, &base_folder, "nested", None).await;

    // Path to nested folder should contain root and parent folder
    let nested_path = Folder::resolve_path(&db, nested_folder.id).await.unwrap();
    assert_eq!(
        &nested_path,
        &[
            FolderPathSegment::from(&root),
            FolderPathSegment::from(&base_folder),
        ]
    );

    // Check arbitrary depth
    const DEPTH: usize = 32;

    let mut folders = vec![root.clone()];
    let mut last_folder = root.clone();

    for i in 0..DEPTH {
        let folder = make_test_folder(&db, &last_folder, format!("test depth {i}"), None).await;
        folders.push(folder.clone());
        last_folder = folder;
    }

    // Get rid of the last one we are operating on
    folders.pop();

    // Path to nested folder should contain root and parent folder
    let nested_path = Folder::resolve_path(&db, last_folder.id).await.unwrap();
    assert_eq!(
        nested_path,
        folders
            .iter()
            .map(FolderPathSegment::from)
            .collect::<Vec<_>>()
    );
}

/// Tests that a folder can be moved to another folder
#[tokio::test]
async fn test_folder_move_to_folder() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, root) = make_test_document_box(&db, "test_1", None).await;

    let base_folder = make_test_folder(&db, &root, "base", None).await;
    let base_folder_2 = make_test_folder(&db, &root, "base_2", None).await;

    assert_eq!(base_folder.folder_id, Some(root.id));

    let base_result = Folder::find_by_id(&db, &document_box.scope, base_folder.id)
        .await
        .unwrap()
        .expect("folder should exist");

    assert_eq!(base_result, base_folder);

    let base_folder = base_folder
        .move_to_folder(&db, base_folder_2.id)
        .await
        .unwrap();

    // Change should be applied to the returned value
    assert_eq!(base_folder.folder_id, Some(base_folder_2.id));

    // Change should also apply to find results
    let base_result = Folder::find_by_id(&db, &document_box.scope, base_folder.id)
        .await
        .unwrap()
        .expect("folder should exist");
    assert_eq!(base_result.folder_id, Some(base_folder_2.id));
}

/// Tests that a folder can be renamed
#[tokio::test]
async fn test_folder_rename() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, root) = make_test_document_box(&db, "test_1", None).await;

    let base_folder = make_test_folder(&db, &root, "base", None).await;
    assert_eq!(base_folder.name, "base");

    let base_result = Folder::find_by_id(&db, &document_box.scope, base_folder.id)
        .await
        .unwrap()
        .expect("folder should exist");

    assert_eq!(base_result, base_folder);

    let base_folder = base_folder.rename(&db, "base_2".to_string()).await.unwrap();

    // Change should be applied to the returned value
    assert_eq!(base_folder.name, "base_2");

    // Change should also apply to find results
    let base_result = Folder::find_by_id(&db, &document_box.scope, base_folder.id)
        .await
        .unwrap()
        .expect("folder should exist");
    assert_eq!(base_result.name, "base_2");
}

/// Tests that a folder can be pinned and unpinned
#[tokio::test]
async fn test_folder_set_pinned() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, root) = make_test_document_box(&db, "test_1", None).await;

    let base_folder = make_test_folder(&db, &root, "base", None).await;
    assert!(!base_folder.pinned);

    let base_result = Folder::find_by_id(&db, &document_box.scope, base_folder.id)
        .await
        .unwrap()
        .expect("folder should exist");

    assert_eq!(base_result, base_folder);

    let base_folder = base_folder.set_pinned(&db, true).await.unwrap();

    // Change should be applied to the returned value
    assert!(base_folder.pinned);

    // Change should also apply to find results
    let base_result = Folder::find_by_id(&db, &document_box.scope, base_folder.id)
        .await
        .unwrap()
        .expect("folder should exist");
    assert!(base_result.pinned);

    let base_folder = base_folder.set_pinned(&db, false).await.unwrap();

    // Change should be applied to the returned value
    assert!(!base_folder.pinned);

    // Change should also apply to find results
    let base_result = Folder::find_by_id(&db, &document_box.scope, base_folder.id)
        .await
        .unwrap()
        .expect("folder should exist");
    assert!(!base_result.pinned);
}

/// Tests that a folder can be found by ID
#[tokio::test]
async fn test_folder_find_by_id() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, root) = make_test_document_box(&db, "test_1", None).await;

    let base_folder = make_test_folder(&db, &root, "base", None).await;

    // Should be able to find the folder
    let base_result = Folder::find_by_id(&db, &document_box.scope, base_folder.id)
        .await
        .unwrap()
        .expect("folder should exist");
    assert_eq!(base_result, base_folder);

    // Unknown folder should return nothing
    let missing_result = Folder::find_by_id(&db, &document_box.scope, Uuid::nil())
        .await
        .unwrap();
    assert!(missing_result.is_none());
}

/// Tests that non-root folders can be queried for
#[tokio::test]
async fn test_folder_all_non_root() {
    let (db, _db_container) = test_tenant_db().await;
    let (_document_box, root) = make_test_document_box(&db, "test_1", None).await;

    let folders = Folder::all_non_root(&db, 0, 5).await.unwrap();
    assert!(folders.is_empty());

    let base_folder_1 = make_test_folder(&db, &root, "base_1", None).await;
    let base_folder_2 = make_test_folder(&db, &root, "base_2", None).await;
    let base_folder_3 = make_test_folder(&db, &root, "base_3", None).await;

    let folders = Folder::all_non_root(&db, 0, 5).await.unwrap();
    assert_eq!(folders.len(), 3);

    assert!(folders.iter().any(|item| item.id == base_folder_1.id));
    assert!(folders.iter().any(|item| item.id == base_folder_2.id));
    assert!(folders.iter().any(|item| item.id == base_folder_3.id));

    let folders = Folder::all_non_root(&db, 0, 1).await.unwrap();
    assert_eq!(folders.len(), 1);
    assert!(folders.iter().any(|item| item.id == base_folder_1.id));

    let folders = Folder::all_non_root(&db, 1, 2).await.unwrap();
    assert_eq!(folders.len(), 2);
    assert!(folders.iter().any(|item| item.id == base_folder_2.id));
    assert!(folders.iter().any(|item| item.id == base_folder_3.id));
}

/// Tests that folders can be found by their parent folder
#[tokio::test]
async fn test_folder_find_by_parent() {
    let (db, _db_container) = test_tenant_db().await;
    let (_document_box, root) = make_test_document_box(&db, "test_1", None).await;

    let folders = Folder::find_by_parent(&db, root.id).await.unwrap();
    assert!(folders.is_empty());

    let base_folder_1 = make_test_folder(&db, &root, "base_1", None).await;
    let base_folder_2 = make_test_folder(&db, &root, "base_2", None).await;
    let base_folder_3 = make_test_folder(&db, &root, "base_3", None).await;

    // Should find the 3 folders within the root
    let folders = Folder::find_by_parent(&db, root.id).await.unwrap();
    assert_eq!(folders.len(), 3);
    assert!(folders.iter().any(|item| item.id == base_folder_1.id));
    assert!(folders.iter().any(|item| item.id == base_folder_2.id));
    assert!(folders.iter().any(|item| item.id == base_folder_3.id));

    // Should find nothing within base folder 3
    let folders = Folder::find_by_parent(&db, base_folder_3.id).await.unwrap();
    assert!(folders.is_empty());

    let base_folder_4 = make_test_folder(&db, &base_folder_3, "base_4", None).await;
    let base_folder_5 = make_test_folder(&db, &base_folder_3, "base_5", None).await;
    let base_folder_6 = make_test_folder(&db, &base_folder_3, "base_6", None).await;

    // Should find the 3 sub folders within base folder 3
    let folders = Folder::find_by_parent(&db, base_folder_3.id).await.unwrap();
    assert_eq!(folders.len(), 3);
    assert!(folders.iter().any(|item| item.id == base_folder_4.id));
    assert!(folders.iter().any(|item| item.id == base_folder_5.id));
    assert!(folders.iter().any(|item| item.id == base_folder_6.id));

    // Freshly created child should have none
    let folders = Folder::find_by_parent(&db, base_folder_6.id).await.unwrap();
    assert!(folders.is_empty());
}

/// Tests that the root folder can be found for a document box
#[tokio::test]
async fn test_folder_find_root() {
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

/// Tests that a folder can be deleted successfully and ensures no other rows are
/// affected and constraints are enforced
#[tokio::test]
async fn test_folder_delete() {
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
    assert!(result.is_restrict());
}

/// Tests that a collection of folders with various scopes and folder IDs can be
/// resolved
#[tokio::test]
async fn test_folder_resolve_with_extra_mixed_scopes() {
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

/// Tests that a collection of folders within the same scope can be resolved by IDs
#[tokio::test]
async fn test_folder_resolve_with_extra() {
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

/// Tests that a folder can be found by ID with extra data
#[tokio::test]
async fn test_folder_find_by_id_with_extra() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, root) = make_test_document_box(&db, "test_1", None).await;

    let base_folder = make_test_folder(&db, &root, "base", None).await;
    let nested_folder = make_test_folder(&db, &base_folder, "base", None).await;

    // Should be able to find the folder
    let base_result = Folder::find_by_id_with_extra(&db, &document_box.scope, base_folder.id)
        .await
        .unwrap()
        .expect("folder should exist");
    assert_eq!(base_result.data.folder, base_folder);
    assert_eq!(base_result.data.created_by, None);
    assert_eq!(base_result.data.last_modified_at, None);
    assert_eq!(base_result.data.last_modified_by, None);
    assert_eq!(&base_result.full_path, &[FolderPathSegment::from(&root)]);

    // Should be able to find the folder
    let nested_result = Folder::find_by_id_with_extra(&db, &document_box.scope, nested_folder.id)
        .await
        .unwrap()
        .expect("folder should exist");
    assert_eq!(nested_result.data.folder, nested_folder);
    assert_eq!(nested_result.data.created_by, None);
    assert_eq!(nested_result.data.last_modified_at, None);
    assert_eq!(nested_result.data.last_modified_by, None);
    assert_eq!(
        &nested_result.full_path,
        &[
            FolderPathSegment::from(&root),
            FolderPathSegment::from(&base_folder)
        ]
    );

    // Unknown folder should return nothing
    let missing_result = Folder::find_by_id_with_extra(&db, &document_box.scope, Uuid::nil())
        .await
        .unwrap();
    assert!(missing_result.is_none());
}

/// Tests that folders can be found by their parent folder with extra data
#[tokio::test]
async fn test_folder_find_by_parent_with_extra() {
    let (db, _db_container) = test_tenant_db().await;
    let (_document_box, root) = make_test_document_box(&db, "test_1", None).await;

    let user = make_test_user(&db, "Test User").await;

    let folders = Folder::find_by_parent_with_extra(&db, root.id)
        .await
        .unwrap();
    assert!(folders.is_empty());

    let base_folder_1 = make_test_folder(&db, &root, "base_1", Some(user.id.clone())).await;
    let base_folder_2 = make_test_folder(&db, &root, "base_2", None).await;
    let base_folder_3 = make_test_folder(&db, &root, "base_3", Some(user.id.clone())).await;

    // Should find the 3 folders within the root
    let folders = Folder::find_by_parent_with_extra(&db, root.id)
        .await
        .unwrap();

    assert_eq!(folders.len(), 3);

    let base_folder_1_result = folders
        .iter()
        .find(|item| item.folder.id == base_folder_1.id)
        .expect("folder should exist");

    assert_eq!(base_folder_1_result.folder, base_folder_1);
    assert_eq!(base_folder_1_result.created_by, Some(user.clone()));
    assert_eq!(base_folder_1_result.last_modified_at, None);
    assert_eq!(base_folder_1_result.last_modified_by, None);

    let base_folder_2_result = folders
        .iter()
        .find(|item| item.folder.id == base_folder_2.id)
        .expect("folder should exist");

    assert_eq!(base_folder_2_result.folder, base_folder_2);
    assert_eq!(base_folder_2_result.created_by, None);
    assert_eq!(base_folder_2_result.last_modified_at, None);
    assert_eq!(base_folder_2_result.last_modified_by, None);

    let base_folder_3_result = folders
        .iter()
        .find(|item| item.folder.id == base_folder_3.id)
        .expect("folder should exist");

    assert_eq!(base_folder_3_result.folder, base_folder_3);
    assert_eq!(base_folder_3_result.created_by, Some(user.clone()));
    assert_eq!(base_folder_3_result.last_modified_at, None);
    assert_eq!(base_folder_3_result.last_modified_by, None);

    // Should find nothing within base folder 3
    let folders = Folder::find_by_parent_with_extra(&db, base_folder_3.id)
        .await
        .unwrap();
    assert!(folders.is_empty());

    let base_folder_4 = make_test_folder(&db, &base_folder_3, "base_4", None).await;
    let base_folder_5 =
        make_test_folder(&db, &base_folder_3, "base_5", Some(user.id.clone())).await;
    let base_folder_6 = make_test_folder(&db, &base_folder_3, "base_6", None).await;

    // Should find the 3 sub folders within base folder 3
    let folders = Folder::find_by_parent_with_extra(&db, base_folder_3.id)
        .await
        .unwrap();
    assert_eq!(folders.len(), 3);

    let base_folder_4_result = folders
        .iter()
        .find(|item| item.folder.id == base_folder_4.id)
        .expect("folder should exist");

    assert_eq!(base_folder_4_result.folder, base_folder_4);
    assert_eq!(base_folder_4_result.created_by, None);
    assert_eq!(base_folder_4_result.last_modified_at, None);
    assert_eq!(base_folder_4_result.last_modified_by, None);

    let base_folder_5_result = folders
        .iter()
        .find(|item| item.folder.id == base_folder_5.id)
        .expect("folder should exist");

    assert_eq!(base_folder_5_result.folder, base_folder_5);
    assert_eq!(base_folder_5_result.created_by, Some(user.clone()));
    assert_eq!(base_folder_5_result.last_modified_at, None);
    assert_eq!(base_folder_5_result.last_modified_by, None);

    let base_folder_6_result = folders
        .iter()
        .find(|item| item.folder.id == base_folder_6.id)
        .expect("folder should exist");

    assert_eq!(base_folder_6_result.folder, base_folder_6);
    assert_eq!(base_folder_6_result.created_by, None);
    assert_eq!(base_folder_6_result.last_modified_at, None);
    assert_eq!(base_folder_6_result.last_modified_by, None);

    // Freshly created child should have none
    let folders = Folder::find_by_parent_with_extra(&db, base_folder_6.id)
        .await
        .unwrap();
    assert!(folders.is_empty());
}

/// Tests that a root folder can be resolved with extra data
#[tokio::test]
async fn test_folder_find_root_with_extra() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, root) = make_test_document_box(&db, "test_1", None).await;

    // Should be able to find valid roots
    let found_root = Folder::find_root_with_extra(&db, &document_box.scope)
        .await
        .unwrap()
        .expect("folder should exist");
    assert_eq!(found_root.data.folder, root);
    assert_eq!(found_root.data.created_by, None);
    assert_eq!(found_root.data.last_modified_at, None);
    assert_eq!(found_root.data.last_modified_by, None);
    assert_eq!(&found_root.full_path, &[]);

    // Shouldn't be able to find a non existent root
    let invalid_root = Folder::find_root(&db, &"test_3".to_string()).await.unwrap();
    assert!(invalid_root.is_none());

    let user = make_test_user(&db, "Test User").await;
    let (document_box, root) = make_test_document_box(&db, "test_2", Some(user.id.clone())).await;

    // Should be able to find valid roots
    let found_root = Folder::find_root_with_extra(&db, &document_box.scope)
        .await
        .unwrap()
        .expect("folder should exist");
    assert_eq!(found_root.data.folder, root);
    assert_eq!(found_root.data.created_by, Some(user.clone()));
    assert_eq!(found_root.data.last_modified_at, None);
    assert_eq!(found_root.data.last_modified_by, None);
    assert_eq!(&found_root.full_path, &[]);

    // Shouldn't be able to find a non existent root
    let invalid_root = Folder::find_root(&db, &"test_3".to_string()).await.unwrap();
    assert!(invalid_root.is_none());
}

/// Tests that the total folder count can be obtained
#[tokio::test]
async fn test_folder_total_count() {
    let (db, _db_container) = test_tenant_db().await;

    // Should be empty initially
    let counts = Folder::total_count(&db).await.unwrap();
    assert_eq!(counts, 0);

    let (_document_box, root) = make_test_document_box(&db, "test", None).await;

    // Will have one folder after the document box is created
    let counts = Folder::total_count(&db).await.unwrap();
    assert_eq!(counts, 1);

    const FOLDER_COUNT: i64 = 10;

    for i in 0..FOLDER_COUNT {
        let _nested_folder = make_test_folder(&db, &root, format!("nested_{i}"), None).await;
    }

    // Should be empty initially
    let counts = Folder::total_count(&db).await.unwrap();
    assert_eq!(
        counts,
        // Additional +1 to include the root folder
        FOLDER_COUNT + 1
    );
}

/// Tests that a folder can be resolved
#[tokio::test]
async fn test_folder_resolved_folder() {
    let (db, _db_container) = test_tenant_db().await;
    let (_document_box, root) = make_test_document_box(&db, "test_1", None).await;

    let resolved = ResolvedFolder::resolve(&db, root.id).await.unwrap();
    assert!(resolved.files.is_empty());
    assert!(resolved.links.is_empty());
    assert!(resolved.folders.is_empty());

    const FOLDER_COUNT: i64 = 10;
    const LINK_COUNT: i64 = 15;
    const FILE_COUNT: i64 = 12;

    let mut folders = Vec::new();

    for i in 0..FOLDER_COUNT {
        folders.push(make_test_folder(&db, &root, format!("nested_{i}"), None).await);
    }

    let mut links = Vec::new();

    for i in 0..LINK_COUNT {
        links.push(make_test_link(&db, &root, format!("Test {i}"), None).await);
    }

    let mut files = Vec::new();

    for i in 0..FILE_COUNT {
        files.push(
            File::create(
                &db,
                CreateFile {
                    id: Uuid::new_v4(),
                    name: format!("File {i}"),
                    folder_id: root.id,
                    mime: "text/plain".to_string(),
                    file_key: "test".to_string(),
                    created_at: Utc::now(),
                    ..Default::default()
                },
            )
            .await
            .unwrap(),
        );
    }

    let resolved = ResolvedFolder::resolve(&db, root.id).await.unwrap();
    assert_eq!(resolved.files.len(), files.len());
    assert_eq!(resolved.links.len(), links.len());
    assert_eq!(resolved.folders.len(), folders.len());

    for folder in folders {
        let resolved = resolved
            .folders
            .iter()
            .find(|item| item.id == folder.id)
            .expect("folder should exist");
        assert_eq!(resolved, &folder);
    }

    for link in links {
        let resolved = resolved
            .links
            .iter()
            .find(|item| item.id == link.id)
            .expect("folder should exist");
        assert_eq!(resolved, &link);
    }

    for file in files {
        let resolved = resolved
            .files
            .iter()
            .find(|item| item.id == file.id)
            .expect("folder should exist");
        assert_eq!(resolved, &file);
    }
}

/// Tests that a folder can be resolved with extra data
#[tokio::test]
async fn test_folder_resolved_folder_with_extra() {
    let (db, _db_container) = test_tenant_db().await;
    let (_document_box, root) = make_test_document_box(&db, "test_1", None).await;

    let resolved = ResolvedFolderWithExtra::resolve(&db, root.id, vec![])
        .await
        .unwrap();
    assert!(resolved.files.is_empty());
    assert!(resolved.links.is_empty());
    assert!(resolved.folders.is_empty());

    const FOLDER_COUNT: i64 = 10;
    const LINK_COUNT: i64 = 15;
    const FILE_COUNT: i64 = 12;

    let mut folders = Vec::new();

    for i in 0..FOLDER_COUNT {
        folders.push(make_test_folder(&db, &root, format!("nested_{i}"), None).await);
    }

    let mut links = Vec::new();

    for i in 0..LINK_COUNT {
        links.push(make_test_link(&db, &root, format!("Test {i}"), None).await);
    }

    let mut files = Vec::new();

    for i in 0..FILE_COUNT {
        files.push(
            File::create(
                &db,
                CreateFile {
                    id: Uuid::new_v4(),
                    name: format!("File {i}"),
                    folder_id: root.id,
                    mime: "text/plain".to_string(),
                    file_key: "test".to_string(),
                    created_at: Utc::now(),
                    ..Default::default()
                },
            )
            .await
            .unwrap(),
        );
    }

    let resolved = ResolvedFolderWithExtra::resolve(&db, root.id, vec![])
        .await
        .unwrap();
    assert_eq!(resolved.files.len(), files.len());
    assert_eq!(resolved.links.len(), links.len());
    assert_eq!(resolved.folders.len(), folders.len());

    for folder in folders {
        let resolved = resolved
            .folders
            .iter()
            .find(|item| item.folder.id == folder.id)
            .expect("folder should exist");
        assert_eq!(&resolved.folder, &folder);

        assert_eq!(resolved.created_by, None);
        assert_eq!(resolved.last_modified_at, None);
        assert_eq!(resolved.last_modified_by, None);
    }

    for link in links {
        let resolved = resolved
            .links
            .iter()
            .find(|item| item.link.id == link.id)
            .expect("folder should exist");
        assert_eq!(&resolved.link, &link);

        assert_eq!(resolved.created_by, None);
        assert_eq!(resolved.last_modified_at, None);
        assert_eq!(resolved.last_modified_by, None);
    }

    for file in files {
        let resolved = resolved
            .files
            .iter()
            .find(|item| item.file.id == file.id)
            .expect("folder should exist");
        assert_eq!(&resolved.file, &file);

        assert_eq!(resolved.created_by, None);
        assert_eq!(resolved.last_modified_at, None);
        assert_eq!(resolved.last_modified_by, None);
    }

    // Create a second document box and redo it with a creator user and a nested folder path
    let (_document_box, root) = make_test_document_box(&db, "test_2", None).await;

    let user = make_test_user(&db, "Test").await;
    let base_folder = make_test_folder(&db, &root, "base", Some(user.id.clone())).await;
    let base_folder_path = Folder::resolve_path(&db, base_folder.id).await.unwrap();

    let resolved = ResolvedFolderWithExtra::resolve(&db, base_folder.id, base_folder_path.clone())
        .await
        .unwrap();
    assert_eq!(&resolved.path, &base_folder_path);
    assert!(resolved.files.is_empty());
    assert!(resolved.links.is_empty());
    assert!(resolved.folders.is_empty());

    let mut folders = Vec::new();

    for i in 0..FOLDER_COUNT {
        folders.push(
            make_test_folder(
                &db,
                &base_folder,
                format!("nested_{i}"),
                Some(user.id.clone()),
            )
            .await,
        );
    }

    let mut links = Vec::new();

    for i in 0..LINK_COUNT {
        links.push(
            make_test_link(
                &db,
                &base_folder,
                format!("Test {i}"),
                Some(user.id.clone()),
            )
            .await,
        );
    }

    let mut files = Vec::new();

    for i in 0..FILE_COUNT {
        files.push(
            File::create(
                &db,
                CreateFile {
                    id: Uuid::new_v4(),
                    name: format!("File {i}"),
                    folder_id: base_folder.id,
                    mime: "text/plain".to_string(),
                    file_key: "test".to_string(),
                    created_at: Utc::now(),
                    created_by: Some(user.id.clone()),
                    ..Default::default()
                },
            )
            .await
            .unwrap(),
        );
    }

    let resolved = ResolvedFolderWithExtra::resolve(&db, base_folder.id, base_folder_path.clone())
        .await
        .unwrap();
    assert_eq!(&resolved.path, &base_folder_path);
    assert_eq!(resolved.files.len(), files.len());
    assert_eq!(resolved.links.len(), links.len());
    assert_eq!(resolved.folders.len(), folders.len());

    for folder in folders {
        let resolved = resolved
            .folders
            .iter()
            .find(|item| item.folder.id == folder.id)
            .expect("folder should exist");
        assert_eq!(&resolved.folder, &folder);

        assert_eq!(resolved.created_by, Some(user.clone()));
        assert_eq!(resolved.last_modified_at, None);
        assert_eq!(resolved.last_modified_by, None);
    }

    for link in links {
        let resolved = resolved
            .links
            .iter()
            .find(|item| item.link.id == link.id)
            .expect("folder should exist");
        assert_eq!(&resolved.link, &link);

        assert_eq!(resolved.created_by, Some(user.clone()));
        assert_eq!(resolved.last_modified_at, None);
        assert_eq!(resolved.last_modified_by, None);
    }

    for file in files {
        let resolved = resolved
            .files
            .iter()
            .find(|item| item.file.id == file.id)
            .expect("folder should exist");
        assert_eq!(&resolved.file, &file);

        assert_eq!(resolved.created_by, Some(user.clone()));
        assert_eq!(resolved.last_modified_at, None);
        assert_eq!(resolved.last_modified_by, None);
    }
}
