use chrono::Utc;
use docbox_database::models::{
    document_box::DocumentBox,
    file::{CreateFile, File},
    folder::{CreateFolder, Folder},
    shared::{DocboxInputPair, FolderPathSegment},
    user::User,
};
use uuid::Uuid;

use crate::common::database::test_tenant_db;

mod common;

#[tokio::test]
async fn test_all_file() {}

#[tokio::test]
async fn test_all_file_by_mime() {}

#[tokio::test]
async fn test_move_to_folder_file() {}

#[tokio::test]
async fn test_rename_file() {}

#[tokio::test]
async fn test_set_pinned_file() {}

#[tokio::test]
async fn test_set_encrypted_file() {}

#[tokio::test]
async fn test_set_mime_file() {}

#[tokio::test]
async fn test_create_file() {}

#[tokio::test]
async fn test_all_convertable_paged_file() {}

#[tokio::test]
async fn test_find_file() {}

#[tokio::test]
async fn test_resolve_path_file() {
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

    assert_eq!(nested_path.len(), 1);

    assert_eq!(nested_path[0].id, root.id);
    assert_eq!(nested_path[0].name, root.name);

    let nested_path = File::resolve_path(&db, nested_file.id).await.unwrap();

    assert_eq!(nested_path.len(), 2);

    assert_eq!(nested_path[0].id, root.id);
    assert_eq!(nested_path[0].name, root.name);

    assert_eq!(nested_path[1].id, base_folder.id);
    assert_eq!(nested_path[1].name, base_folder.name);
}

#[tokio::test]
async fn test_find_by_parent_file() {}

#[tokio::test]
async fn test_delete_file() {}

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
async fn test_total_count_file() {}

#[tokio::test]
async fn test_total_size_file() {}

#[tokio::test]
async fn test_total_size_within_scope_file() {}
