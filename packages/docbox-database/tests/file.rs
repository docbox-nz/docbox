use chrono::Utc;
use docbox_database::{
    models::{
        document_box::DocumentBox,
        file::{CreateFile, File},
        folder::{CreateFolder, Folder},
        shared::{DocboxInputPair, FolderPathSegment},
        user::User,
    },
    utils::DatabaseErrorExt,
};
use uuid::Uuid;

use crate::common::{
    database::test_tenant_db, make_test_document_box, make_test_file, make_test_file_type,
    make_test_folder,
};

mod common;

#[tokio::test]
async fn test_file_create() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, root) = make_test_document_box(&db, "test", None).await;

    let file = File::create(
        &db,
        CreateFile {
            id: Uuid::new_v4(),
            name: "test".to_string(),
            folder_id: root.id,
            size: 1,
            parent_id: None,
            mime: "text/plain".to_string(),
            hash: "aaffbb".to_string(),
            file_key: "test/key".to_string(),
            created_by: None,
            created_at: Utc::now(),
            encrypted: true,
        },
    )
    .await
    .unwrap();

    assert_eq!(file.name, "test");
    assert_eq!(file.mime, "text/plain");
    assert_eq!(file.hash, "aaffbb");
    assert_eq!(file.file_key, "test/key");
    assert_eq!(file.folder_id, root.id);
    assert_eq!(file.created_by, None);
    assert_eq!(file.parent_id, None);
    assert_eq!(file.size, 1);
    assert!(file.encrypted);

    let result = File::find(&db, &document_box.scope, file.id)
        .await
        .unwrap()
        .expect("file should exist");
    assert_eq!(result, file);
}

#[tokio::test]
async fn test_file_all() {
    let (db, _db_container) = test_tenant_db().await;
    let (_document_box, root) = make_test_document_box(&db, "test_1", None).await;

    let files = File::all(&db, 0, 5).await.unwrap();
    assert!(files.is_empty());

    let file_1 = make_test_file(&db, &root, "Test 1", None).await;
    let file_2 = make_test_file(&db, &root, "Test 2", None).await;
    let file_3 = make_test_file(&db, &root, "Test 3", None).await;

    let files = File::all(&db, 0, 5).await.unwrap();
    assert_eq!(files.len(), 3);
    assert!(files.iter().any(|item| item.file.id == file_1.id));
    assert!(files.iter().any(|item| item.file.id == file_2.id));
    assert!(files.iter().any(|item| item.file.id == file_3.id));
}

#[tokio::test]
async fn test_file_all_by_mime() {
    let (db, _db_container) = test_tenant_db().await;
    let (_document_box, root) = make_test_document_box(&db, "test_1", None).await;

    let files = File::all_by_mime(&db, "text/plain", 0, 5).await.unwrap();
    assert!(files.is_empty());

    let file_1 = make_test_file_type(&db, &root, "Test 1", "text/plain", None).await;
    let file_2 = make_test_file_type(&db, &root, "Test 2", "text/plain", None).await;
    let file_3 = make_test_file_type(&db, &root, "Test 3", "text/plain", None).await;

    let files = File::all_by_mime(&db, "text/plain", 0, 5).await.unwrap();
    assert_eq!(files.len(), 3);
    assert!(files.iter().any(|item| item.file.id == file_1.id));
    assert!(files.iter().any(|item| item.file.id == file_2.id));
    assert!(files.iter().any(|item| item.file.id == file_3.id));

    let files = File::all_by_mime(&db, "text/other", 0, 5).await.unwrap();
    assert!(files.is_empty());
}

#[tokio::test]
async fn test_file_all_by_mimes() {
    let (db, _db_container) = test_tenant_db().await;
    let (_document_box, root) = make_test_document_box(&db, "test_1", None).await;

    let files = File::all_by_mimes(&db, &["text/plain", "text/plain2"], 0, 5)
        .await
        .unwrap();
    assert!(files.is_empty());

    let file_1 = make_test_file_type(&db, &root, "Test 1", "text/plain", None).await;
    let file_2 = make_test_file_type(&db, &root, "Test 2", "text/plain", None).await;
    let file_3 = make_test_file_type(&db, &root, "Test 3", "text/plain2", None).await;

    let files = File::all_by_mimes(&db, &["text/plain", "text/plain2"], 0, 5)
        .await
        .unwrap();
    assert_eq!(files.len(), 3);
    assert!(files.iter().any(|item| item.file.id == file_1.id));
    assert!(files.iter().any(|item| item.file.id == file_2.id));
    assert!(files.iter().any(|item| item.file.id == file_3.id));

    let files = File::all_by_mimes(&db, &["text/plain"], 0, 5)
        .await
        .unwrap();
    assert_eq!(files.len(), 2);
    assert!(files.iter().any(|item| item.file.id == file_1.id));
    assert!(files.iter().any(|item| item.file.id == file_2.id));

    let files = File::all_by_mimes(&db, &["text/plain2"], 0, 5)
        .await
        .unwrap();
    assert_eq!(files.len(), 1);
    assert!(files.iter().any(|item| item.file.id == file_3.id));

    let files = File::all_by_mimes(&db, &["text/unknown"], 0, 5)
        .await
        .unwrap();
    assert!(files.is_empty());
}

#[tokio::test]
async fn test_file_move_to_folder() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, root) = make_test_document_box(&db, "test_1", None).await;

    let base_file = make_test_file(&db, &root, "base", None).await;
    let base_folder = make_test_folder(&db, &root, "base_2", None).await;

    assert_eq!(base_file.folder_id, root.id);

    let base_result = File::find(&db, &document_box.scope, base_file.id)
        .await
        .unwrap()
        .expect("file should exist");

    assert_eq!(base_result, base_file);

    let base_file = base_file.move_to_folder(&db, base_folder.id).await.unwrap();

    // Change should be applied to the returned value
    assert_eq!(base_file.folder_id, base_folder.id);

    // Change should also apply to find results
    let base_result = File::find(&db, &document_box.scope, base_file.id)
        .await
        .unwrap()
        .expect("file should exist");
    assert_eq!(base_result.folder_id, base_folder.id);
}

#[tokio::test]
async fn test_file_rename() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, root) = make_test_document_box(&db, "test_1", None).await;

    let base_file = make_test_file(&db, &root, "base", None).await;
    assert_eq!(base_file.name, "base");

    let base_result = File::find(&db, &document_box.scope, base_file.id)
        .await
        .unwrap()
        .expect("file should exist");

    assert_eq!(base_result, base_file);

    let base_file = base_file.rename(&db, "base_2".to_string()).await.unwrap();

    // Change should be applied to the returned value
    assert_eq!(base_file.name, "base_2");

    // Change should also apply to find results
    let base_result = File::find(&db, &document_box.scope, base_file.id)
        .await
        .unwrap()
        .expect("file should exist");
    assert_eq!(base_result.name, "base_2");
}

#[tokio::test]
async fn test_file_set_pinned() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, root) = make_test_document_box(&db, "test_1", None).await;

    let base_file = make_test_file(&db, &root, "base", None).await;
    assert!(!base_file.pinned);

    let base_result = File::find(&db, &document_box.scope, base_file.id)
        .await
        .unwrap()
        .expect("file should exist");

    assert_eq!(base_result, base_file);

    let base_file = base_file.set_pinned(&db, true).await.unwrap();

    // Change should be applied to the returned value
    assert!(base_file.pinned);

    // Change should also apply to find results
    let base_result = File::find(&db, &document_box.scope, base_file.id)
        .await
        .unwrap()
        .expect("file should exist");
    assert!(base_result.pinned);

    let base_file = base_file.set_pinned(&db, false).await.unwrap();

    // Change should be applied to the returned value
    assert!(!base_file.pinned);

    // Change should also apply to find results
    let base_result = File::find(&db, &document_box.scope, base_file.id)
        .await
        .unwrap()
        .expect("file should exist");
    assert!(!base_result.pinned);
}

#[tokio::test]
async fn test_file_set_encrypted() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, root) = make_test_document_box(&db, "test_1", None).await;

    let base_file = make_test_file(&db, &root, "base", None).await;
    assert!(!base_file.encrypted);

    let base_result = File::find(&db, &document_box.scope, base_file.id)
        .await
        .unwrap()
        .expect("file should exist");

    assert_eq!(base_result, base_file);

    let base_file = base_file.set_encrypted(&db, true).await.unwrap();

    // Change should be applied to the returned value
    assert!(base_file.encrypted);

    // Change should also apply to find results
    let base_result = File::find(&db, &document_box.scope, base_file.id)
        .await
        .unwrap()
        .expect("file should exist");
    assert!(base_result.encrypted);

    let base_file = base_file.set_encrypted(&db, false).await.unwrap();

    // Change should be applied to the returned value
    assert!(!base_file.encrypted);

    // Change should also apply to find results
    let base_result = File::find(&db, &document_box.scope, base_file.id)
        .await
        .unwrap()
        .expect("file should exist");
    assert!(!base_result.encrypted);
}

#[tokio::test]
async fn test_file_set_mime() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, root) = make_test_document_box(&db, "test_1", None).await;

    let base_file = File::create(
        &db,
        CreateFile {
            id: Uuid::new_v4(),
            name: "Root".to_string(),
            folder_id: root.id,
            mime: "text/plain".to_string(),
            file_key: "test".to_string(),
            created_at: Utc::now(),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    assert_eq!(base_file.mime, "text/plain");

    let base_result = File::find(&db, &document_box.scope, base_file.id)
        .await
        .unwrap()
        .expect("file should exist");

    assert_eq!(base_result, base_file);

    let base_file = base_file
        .set_mime(&db, "test/mime".to_string())
        .await
        .unwrap();

    // Change should be applied to the returned value
    assert_eq!(base_file.mime, "test/mime");

    // Change should also apply to find results
    let base_result = File::find(&db, &document_box.scope, base_file.id)
        .await
        .unwrap()
        .expect("file should exist");
    assert_eq!(base_result.mime, "test/mime");
}

#[tokio::test]
async fn test_find_file() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, root) = make_test_document_box(&db, "test", None).await;

    let file = make_test_file(&db, &root, "Test File", None).await;
    let result = File::find(&db, &document_box.scope, file.id)
        .await
        .unwrap()
        .expect("file should exist");
    assert_eq!(result, file);

    let result = File::find(&db, &document_box.scope, Uuid::nil())
        .await
        .unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_resolve_path_file() {
    let (db, _db_container) = test_tenant_db().await;
    let (_document_box, root) = make_test_document_box(&db, "test_1", None).await;

    let root_file = File::create(
        &db,
        CreateFile {
            id: Uuid::new_v4(),
            name: "Root".to_string(),
            folder_id: root.id,
            mime: "text/plain".to_string(),
            file_key: "test".to_string(),
            created_at: Utc::now(),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let base_folder = make_test_folder(&db, &root, "base", None).await;

    let nested_file = File::create(
        &db,
        CreateFile {
            id: Uuid::new_v4(),
            name: "Nested".to_string(),
            folder_id: base_folder.id,
            mime: "text/plain".to_string(),
            file_key: "test".to_string(),
            created_at: Utc::now(),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let nested_path = File::resolve_path(&db, root_file.id).await.unwrap();
    assert_eq!(&nested_path, &[FolderPathSegment::from(&root)]);

    let nested_path = File::resolve_path(&db, nested_file.id).await.unwrap();
    assert_eq!(
        &nested_path,
        &[
            FolderPathSegment::from(&root),
            FolderPathSegment::from(&base_folder)
        ]
    );
}

#[tokio::test]
async fn test_find_by_parent_file() {
    let (db, _db_container) = test_tenant_db().await;
    let (_document_box, root) = make_test_document_box(&db, "test_1", None).await;

    let files = File::find_by_parent(&db, root.id).await.unwrap();
    assert!(files.is_empty());

    let file_1 = make_test_file(&db, &root, "Test 1", None).await;
    let file_2 = make_test_file(&db, &root, "Test 2", None).await;
    let file_3 = make_test_file(&db, &root, "Test 3", None).await;

    let files = File::find_by_parent(&db, root.id).await.unwrap();
    assert_eq!(files.len(), 3);
    assert!(files.iter().any(|item| item.id == file_1.id));
    assert!(files.iter().any(|item| item.id == file_2.id));
    assert!(files.iter().any(|item| item.id == file_3.id));

    let base_folder_1 = make_test_folder(&db, &root, "base_1", None).await;
    let files = File::find_by_parent(&db, base_folder_1.id).await.unwrap();
    assert!(files.is_empty());

    let file_4 = make_test_file(&db, &base_folder_1, "Test 4", None).await;
    let file_5 = make_test_file(&db, &base_folder_1, "Test 5", None).await;
    let file_6 = make_test_file(&db, &base_folder_1, "Test 6", None).await;

    let files = File::find_by_parent(&db, base_folder_1.id).await.unwrap();
    assert_eq!(files.len(), 3);
    assert!(files.iter().any(|item| item.id == file_4.id));
    assert!(files.iter().any(|item| item.id == file_5.id));
    assert!(files.iter().any(|item| item.id == file_6.id));
}

#[tokio::test]
async fn test_delete_file() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, root) = make_test_document_box(&db, "test_1", None).await;

    let file = make_test_file(&db, &root, "test", None).await;
    let other_file = make_test_file(&db, &root, "test_2", None).await;

    // File should exist
    let target = File::find(&db, &document_box.scope, file.id).await.unwrap();
    assert!(target.is_some());

    // Delete file should delete one row
    let result = file.delete(&db).await.unwrap();
    assert_eq!(result.rows_affected(), 1);

    // File shouldn't exist
    let target = File::find(&db, &document_box.scope, file.id).await.unwrap();
    assert!(target.is_none());

    // Other file should still exist
    let target = File::find(&db, &document_box.scope, other_file.id)
        .await
        .unwrap();
    assert!(target.is_some());

    // Delete file shouldn't delete any rows now that its gone
    let result = file.delete(&db).await.unwrap();
    assert_eq!(result.rows_affected(), 0);

    // Should not be able to delete the root while another file is still present
    // (Enforce proper deletion)
    let result = root.delete(&db).await.unwrap_err();
    assert!(result.is_restrict());
}

/// Tests that [`File::resolve_with_extra`] can locate a collection of files using
/// a scope and collection of file IDS
#[tokio::test]
async fn test_resolve_with_extra_file() {
    let (db, _db_container) = test_tenant_db().await;

    let scope_1 = "test_1".to_string();

    _ = DocumentBox::create(&db, scope_1.clone()).await.unwrap();

    let root_1 = Folder::create(
        &db,
        CreateFolder {
            name: "Root".to_string(),
            document_box: scope_1.to_string(),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let root_2 = Folder::create(
        &db,
        CreateFolder {
            name: "Root 2".to_string(),
            document_box: scope_1.to_string(),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let nested = Folder::create(
        &db,
        CreateFolder {
            name: "Nested".to_string(),
            document_box: root_2.document_box.clone(),
            folder_id: Some(root_2.id),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let file_1 = File::create(
        &db,
        CreateFile {
            id: Uuid::new_v4(),
            name: "File 1".to_string(),
            folder_id: root_1.id,
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let user = User::store(
        &db,
        "test".to_string(),
        Some("Test User".to_string()),
        Some("image.png".to_string()),
    )
    .await
    .unwrap();

    let file_2 = File::create(
        &db,
        CreateFile {
            id: Uuid::new_v4(),
            name: "File 2".to_string(),
            folder_id: root_2.id,
            created_by: Some(user.id.clone()),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let file_3 = File::create(
        &db,
        CreateFile {
            id: Uuid::new_v4(),
            name: "File 3".to_string(),
            folder_id: nested.id,
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let file_4 = File::create(
        &db,
        CreateFile {
            id: Uuid::new_v4(),
            name: "File 4".to_string(),
            folder_id: nested.id,
            created_by: Some(user.id.clone()),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let files = File::resolve_with_extra(
        &db,
        &scope_1,
        vec![file_1.id, file_2.id, file_3.id, file_4.id],
    )
    .await
    .unwrap();

    // Should be able to find the file in the first scope
    let file = files
        .iter()
        .find(|file| file.data.file.id == file_1.id)
        .expect("file should exist");

    assert_eq!(file.data.file, file_1);
    assert_eq!(file.data.last_modified_at, None);
    assert_eq!(file.data.last_modified_by, None);
    assert_eq!(file.data.created_by, None);
    assert_eq!(
        file.full_path,
        vec![FolderPathSegment {
            id: root_1.id,
            name: root_1.name.clone(),
        }]
    );

    // Should locate and lookup the created_by user for the second file
    let file = files
        .iter()
        .find(|file| file.data.file.id == file_2.id)
        .expect("file should exist");

    assert_eq!(file.data.file, file_2);
    assert_eq!(file.data.last_modified_at, None);
    assert_eq!(file.data.last_modified_by, None);
    assert_eq!(file.data.created_by, Some(user.clone()));
    assert_eq!(
        file.full_path,
        vec![FolderPathSegment {
            id: root_2.id,
            name: root_2.name.clone(),
        }]
    );

    // Should locate the third file and resolve the nested path
    let file = files
        .iter()
        .find(|file| file.data.file.id == file_3.id)
        .expect("file should exist");

    assert_eq!(file.data.file, file_3);
    assert_eq!(file.data.last_modified_at, None);
    assert_eq!(file.data.last_modified_by, None);
    assert_eq!(file.data.created_by, None);
    assert_eq!(
        file.full_path,
        vec![
            FolderPathSegment {
                id: root_2.id,
                name: root_2.name.clone(),
            },
            FolderPathSegment {
                id: nested.id,
                name: nested.name.clone(),
            }
        ]
    );

    // Should locate the third file and resolve the nested path and the creator
    let file = files
        .iter()
        .find(|file| file.data.file.id == file_4.id)
        .expect("file should exist");

    assert_eq!(file.data.file, file_4);
    assert_eq!(file.data.last_modified_at, None);
    assert_eq!(file.data.last_modified_by, None);
    assert_eq!(file.data.created_by, Some(user.clone()));
    assert_eq!(
        file.full_path,
        vec![
            FolderPathSegment {
                id: root_2.id,
                name: root_2.name.clone(),
            },
            FolderPathSegment {
                id: nested.id,
                name: nested.name.clone(),
            }
        ]
    );
}

/// Tests that [`File::resolve_with_extra_mixed_scopes`] can locate a collection of files using
/// a collection of scope and file ID pairs
#[tokio::test]
async fn test_resolve_with_extra_mixed_scopes_file() {
    let (db, _db_container) = test_tenant_db().await;

    let scope_1 = "test_1".to_string();
    let scope_2 = "test_2".to_string();

    _ = DocumentBox::create(&db, scope_1.clone()).await.unwrap();
    _ = DocumentBox::create(&db, scope_2.clone()).await.unwrap();

    let root_1 = Folder::create(
        &db,
        CreateFolder {
            name: "Root".to_string(),
            document_box: scope_1.to_string(),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let root_2 = Folder::create(
        &db,
        CreateFolder {
            name: "Root".to_string(),
            document_box: scope_2.to_string(),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let nested = Folder::create(
        &db,
        CreateFolder {
            name: "Nested".to_string(),
            document_box: root_2.document_box.clone(),
            folder_id: Some(root_2.id),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let file_1 = File::create(
        &db,
        CreateFile {
            id: Uuid::new_v4(),
            name: "File 1".to_string(),
            folder_id: root_1.id,
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let user = User::store(
        &db,
        "test".to_string(),
        Some("Test User".to_string()),
        Some("image.png".to_string()),
    )
    .await
    .unwrap();

    let file_2 = File::create(
        &db,
        CreateFile {
            id: Uuid::new_v4(),
            name: "File 2".to_string(),
            folder_id: root_2.id,
            created_by: Some(user.id.clone()),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let file_3 = File::create(
        &db,
        CreateFile {
            id: Uuid::new_v4(),
            name: "File 3".to_string(),
            folder_id: nested.id,
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let file_4 = File::create(
        &db,
        CreateFile {
            id: Uuid::new_v4(),
            name: "File 4".to_string(),
            folder_id: nested.id,
            created_by: Some(user.id.clone()),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let files = File::resolve_with_extra_mixed_scopes(
        &db,
        vec![
            DocboxInputPair::new(&scope_1, file_1.id),
            DocboxInputPair::new(&scope_2, file_2.id),
            DocboxInputPair::new(&scope_2, file_3.id),
            DocboxInputPair::new(&scope_2, file_4.id),
        ],
    )
    .await
    .unwrap();

    // Should be able to find the file in the first scope
    let file = files
        .iter()
        .find(|file| file.data.file.id == file_1.id)
        .expect("file should exist");

    assert_eq!(file.data.file, file_1);
    assert_eq!(file.data.last_modified_at, None);
    assert_eq!(file.data.last_modified_by, None);
    assert_eq!(file.data.created_by, None);
    assert_eq!(file.document_box, scope_1);
    assert_eq!(
        file.full_path,
        vec![FolderPathSegment {
            id: root_1.id,
            name: root_1.name.clone(),
        }]
    );

    // Should locate and lookup the created_by user for the second file
    let file = files
        .iter()
        .find(|file| file.data.file.id == file_2.id)
        .expect("file should exist");

    assert_eq!(file.data.file, file_2);
    assert_eq!(file.data.last_modified_at, None);
    assert_eq!(file.data.last_modified_by, None);
    assert_eq!(file.data.created_by, Some(user.clone()));
    assert_eq!(file.document_box, scope_2);
    assert_eq!(
        file.full_path,
        vec![FolderPathSegment {
            id: root_2.id,
            name: root_2.name.clone(),
        }]
    );

    // Should locate the third file and resolve the nested path
    let file = files
        .iter()
        .find(|file| file.data.file.id == file_3.id)
        .expect("file should exist");

    assert_eq!(file.data.file, file_3);
    assert_eq!(file.data.last_modified_at, None);
    assert_eq!(file.data.last_modified_by, None);
    assert_eq!(file.data.created_by, None);
    assert_eq!(file.document_box, scope_2);
    assert_eq!(
        file.full_path,
        vec![
            FolderPathSegment {
                id: root_2.id,
                name: root_2.name.clone(),
            },
            FolderPathSegment {
                id: nested.id,
                name: nested.name.clone(),
            }
        ]
    );

    // Should locate the third file and resolve the nested path and the creator
    let file = files
        .iter()
        .find(|file| file.data.file.id == file_4.id)
        .expect("file should exist");

    assert_eq!(file.data.file, file_4);
    assert_eq!(file.data.last_modified_at, None);
    assert_eq!(file.data.last_modified_by, None);
    assert_eq!(file.data.created_by, Some(user.clone()));
    assert_eq!(file.document_box, scope_2);
    assert_eq!(
        file.full_path,
        vec![
            FolderPathSegment {
                id: root_2.id,
                name: root_2.name.clone(),
            },
            FolderPathSegment {
                id: nested.id,
                name: nested.name.clone(),
            }
        ]
    );
}

/// Tests that [`File::find_with_extra`] can locate files by ID and scope
/// obtain the requested additional extra details about the file
#[tokio::test]
async fn test_find_with_extra_file() {
    let (db, _db_container) = test_tenant_db().await;

    let scope_1 = "test_1".to_string();
    let scope_2 = "test_2".to_string();

    _ = DocumentBox::create(&db, scope_1.clone()).await.unwrap();
    _ = DocumentBox::create(&db, scope_2.clone()).await.unwrap();

    let root_1 = Folder::create(
        &db,
        CreateFolder {
            name: "Root".to_string(),
            document_box: scope_1.to_string(),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let root_2 = Folder::create(
        &db,
        CreateFolder {
            name: "Root".to_string(),
            document_box: scope_2.to_string(),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let file_1 = File::create(
        &db,
        CreateFile {
            id: Uuid::new_v4(),
            name: "File 1".to_string(),
            folder_id: root_1.id,
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let user = User::store(
        &db,
        "test".to_string(),
        Some("Test User".to_string()),
        Some("image.png".to_string()),
    )
    .await
    .unwrap();

    let file_2 = File::create(
        &db,
        CreateFile {
            id: Uuid::new_v4(),
            name: "File 2".to_string(),
            folder_id: root_2.id,
            created_by: Some(user.id.clone()),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // Should be able to find the file in the first scope
    let file = File::find_with_extra(&db, &scope_1, file_1.id)
        .await
        .unwrap()
        .expect("file should exist");

    assert_eq!(file.file, file_1);
    assert_eq!(file.last_modified_at, None);
    assert_eq!(file.last_modified_by, None);
    assert_eq!(file.created_by, None);

    // Searching for the unknown file in the second scope should result in nothing
    let file = File::find_with_extra(&db, &scope_2, file_1.id)
        .await
        .unwrap();
    assert!(file.is_none());

    // Should locate and lookup the created_by user for the second file
    let file = File::find_with_extra(&db, &scope_2, file_2.id)
        .await
        .unwrap()
        .expect("file should exist");

    assert_eq!(file.file, file_2);
    assert_eq!(file.last_modified_at, None);
    assert_eq!(file.last_modified_by, None);
    assert_eq!(file.created_by, Some(user.clone()));
}

/// Tests that [`File::find_by_parent_folder_with_extra`] can locate files using their parent folder and
/// obtain the requested additional extra details about the files
#[tokio::test]
async fn test_find_by_parent_folder_with_extra_file() {
    let (db, _db_container) = test_tenant_db().await;

    let scope_1 = "test_1".to_string();

    _ = DocumentBox::create(&db, scope_1.clone()).await.unwrap();

    let root_1 = Folder::create(
        &db,
        CreateFolder {
            name: "Root".to_string(),
            document_box: scope_1.to_string(),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let file_1 = File::create(
        &db,
        CreateFile {
            id: Uuid::new_v4(),
            name: "File 1".to_string(),
            folder_id: root_1.id,
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let user = User::store(
        &db,
        "test".to_string(),
        Some("Test User".to_string()),
        Some("image.png".to_string()),
    )
    .await
    .unwrap();

    let file_2 = File::create(
        &db,
        CreateFile {
            id: Uuid::new_v4(),
            name: "File 2".to_string(),
            folder_id: root_1.id,
            created_by: Some(user.id.clone()),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let nested = Folder::create(
        &db,
        CreateFolder {
            name: "Nested".to_string(),
            document_box: root_1.document_box.clone(),
            folder_id: Some(root_1.id),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let files = File::find_by_parent_folder_with_extra(&db, nested.id)
        .await
        .unwrap();

    assert!(files.is_empty());

    let file_3 = File::create(
        &db,
        CreateFile {
            id: Uuid::new_v4(),
            name: "File 3".to_string(),
            folder_id: nested.id,
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let file_4 = File::create(
        &db,
        CreateFile {
            id: Uuid::new_v4(),
            name: "File 4".to_string(),
            folder_id: nested.id,
            created_by: Some(user.id.clone()),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let files = File::find_by_parent_folder_with_extra(&db, root_1.id)
        .await
        .unwrap();

    let file = files
        .iter()
        .find(|file| file.file.id == file_1.id)
        .expect("file should exist");

    assert_eq!(file.file, file_1);
    assert_eq!(file.last_modified_at, None);
    assert_eq!(file.last_modified_by, None);
    assert_eq!(file.created_by, None);

    let file = files
        .iter()
        .find(|file| file.file.id == file_2.id)
        .expect("file should exist");

    assert_eq!(file.file, file_2);
    assert_eq!(file.last_modified_at, None);
    assert_eq!(file.last_modified_by, None);
    assert_eq!(file.created_by, Some(user.clone()));

    let files = File::find_by_parent_folder_with_extra(&db, nested.id)
        .await
        .unwrap();

    let file = files
        .iter()
        .find(|file| file.file.id == file_3.id)
        .expect("file should exist");

    assert_eq!(file.file, file_3);
    assert_eq!(file.last_modified_at, None);
    assert_eq!(file.last_modified_by, None);
    assert_eq!(file.created_by, None);

    let file = files
        .iter()
        .find(|file| file.file.id == file_4.id)
        .expect("file should exist");

    assert_eq!(file.file, file_4);
    assert_eq!(file.last_modified_at, None);
    assert_eq!(file.last_modified_by, None);
    assert_eq!(file.created_by, Some(user.clone()));
}

/// Tests that [`File::find_by_parent_file_with_extra`] can locate files using their parent file and
/// obtain the requested additional extra details about the files
#[tokio::test]
async fn test_find_by_parent_file_with_extra_file() {
    let (db, _db_container) = test_tenant_db().await;

    let scope_1 = "test_1".to_string();

    _ = DocumentBox::create(&db, scope_1.clone()).await.unwrap();

    let root_1 = Folder::create(
        &db,
        CreateFolder {
            name: "Root".to_string(),
            document_box: scope_1.to_string(),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let file_1 = File::create(
        &db,
        CreateFile {
            id: Uuid::new_v4(),
            name: "File 1".to_string(),
            folder_id: root_1.id,
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let files = File::find_by_parent_file_with_extra(&db, file_1.id)
        .await
        .unwrap();

    assert!(files.is_empty());

    let user = User::store(
        &db,
        "test".to_string(),
        Some("Test User".to_string()),
        Some("image.png".to_string()),
    )
    .await
    .unwrap();

    let file_2 = File::create(
        &db,
        CreateFile {
            id: Uuid::new_v4(),
            name: "File 2".to_string(),
            parent_id: Some(file_1.id),
            folder_id: root_1.id,
            created_by: Some(user.id.clone()),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let file_3 = File::create(
        &db,
        CreateFile {
            id: Uuid::new_v4(),
            name: "File 3".to_string(),
            parent_id: Some(file_1.id),
            folder_id: root_1.id,
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let file_4 = File::create(
        &db,
        CreateFile {
            id: Uuid::new_v4(),
            name: "File 4".to_string(),
            parent_id: Some(file_1.id),
            folder_id: root_1.id,
            created_by: Some(user.id.clone()),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let files = File::find_by_parent_file_with_extra(&db, file_1.id)
        .await
        .unwrap();

    let file = files
        .iter()
        .find(|file| file.file.id == file_2.id)
        .expect("file should exist");

    assert_eq!(file.file, file_2);
    assert_eq!(file.last_modified_at, None);
    assert_eq!(file.last_modified_by, None);
    assert_eq!(file.created_by, Some(user.clone()));

    let file = files
        .iter()
        .find(|file| file.file.id == file_3.id)
        .expect("file should exist");

    assert_eq!(file.file, file_3);
    assert_eq!(file.last_modified_at, None);
    assert_eq!(file.last_modified_by, None);
    assert_eq!(file.created_by, None);

    let file = files
        .iter()
        .find(|file| file.file.id == file_4.id)
        .expect("file should exist");

    assert_eq!(file.file, file_4);
    assert_eq!(file.last_modified_at, None);
    assert_eq!(file.last_modified_by, None);
    assert_eq!(file.created_by, Some(user.clone()));
}

#[tokio::test]
async fn test_total_count_file() {
    let (db, _db_container) = test_tenant_db().await;
    let (_document_box, root) = make_test_document_box(&db, "test_1", None).await;

    let count = File::total_count(&db).await.unwrap();
    assert_eq!(count, 0);

    const FILE_COUNT: i64 = 15;

    for i in 0..FILE_COUNT {
        make_test_file(&db, &root, format!("Test {i}"), None).await;
    }

    let count = File::total_count(&db).await.unwrap();
    assert_eq!(count, FILE_COUNT);
}

#[tokio::test]
async fn test_total_size_file() {
    let (db, _db_container) = test_tenant_db().await;
    let (_document_box, root) = make_test_document_box(&db, "test_1", None).await;

    let count = File::total_size(&db).await.unwrap();
    assert_eq!(count, 0);

    const FILE_COUNT: i64 = 15;
    const FILE_SIZE: i32 = 150;

    for i in 0..FILE_COUNT {
        File::create(
            &db,
            CreateFile {
                id: Uuid::new_v4(),
                name: format!("Test {i}"),
                folder_id: root.id,
                size: FILE_SIZE,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    }

    let count = File::total_size(&db).await.unwrap();
    assert_eq!(count, FILE_COUNT * (FILE_SIZE as i64));
}

#[tokio::test]
async fn test_total_size_within_scope_file() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box_1, root_1) = make_test_document_box(&db, "test_1", None).await;
    let (document_box_2, root_2) = make_test_document_box(&db, "test_2", None).await;

    let count = File::total_size_within_scope(&db, &document_box_1.scope)
        .await
        .unwrap();
    assert_eq!(count, 0);
    let count = File::total_size_within_scope(&db, &document_box_2.scope)
        .await
        .unwrap();
    assert_eq!(count, 0);

    const FILE_COUNT: i64 = 15;
    const FILE_SIZE: i32 = 150;

    for i in 0..FILE_COUNT {
        File::create(
            &db,
            CreateFile {
                id: Uuid::new_v4(),
                name: format!("Test {i}"),
                folder_id: root_1.id,
                size: FILE_SIZE,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    }

    let count = File::total_size_within_scope(&db, &document_box_1.scope)
        .await
        .unwrap();
    assert_eq!(count, FILE_COUNT * (FILE_SIZE as i64));

    let count = File::total_size_within_scope(&db, &document_box_2.scope)
        .await
        .unwrap();
    assert_eq!(count, 0);

    for i in 0..FILE_COUNT {
        File::create(
            &db,
            CreateFile {
                id: Uuid::new_v4(),
                name: format!("Test {i}"),
                folder_id: root_2.id,
                size: FILE_SIZE,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    }

    let count = File::total_size_within_scope(&db, &document_box_2.scope)
        .await
        .unwrap();
    assert_eq!(count, FILE_COUNT * (FILE_SIZE as i64));
}
