use chrono::Utc;
use docbox_database::models::{
    document_box::DocumentBox,
    file::{CreateFile, File},
    folder::{CreateFolder, Folder},
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
            created_by: None,
            parent_id: None,
            mime: "text/plain".to_string(),
            hash: Default::default(),
            size: 0,
            file_key: "test".to_string(),
            created_at: Utc::now(),
            encrypted: false,
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
            created_by: None,
            parent_id: None,
            mime: "text/plain".to_string(),
            hash: Default::default(),
            size: 0,
            file_key: "test".to_string(),
            created_at: Utc::now(),
            encrypted: false,
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

#[tokio::test]
async fn test_resolve_with_extra_file() {}

#[tokio::test]
async fn test_resolve_with_extra_mixed_scopes_file() {}

#[tokio::test]
async fn test_find_with_extra_file() {}

#[tokio::test]
async fn test_find_by_parent_folder_with_extra_file() {}

#[tokio::test]
async fn test_find_by_parent_file_with_extra_file() {}

#[tokio::test]
async fn test_total_count_file() {}

#[tokio::test]
async fn test_total_size_file() {}

#[tokio::test]
async fn test_total_size_within_scope_file() {}
