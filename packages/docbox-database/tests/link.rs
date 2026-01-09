use docbox_database::models::{
    document_box::DocumentBox,
    folder::{CreateFolder, Folder},
    link::{CreateLink, Link},
};

use crate::common::database::test_tenant_db;

mod common;

#[tokio::test]
async fn test_create_link() {}

#[tokio::test]
async fn test_move_link_to_folder() {}

#[tokio::test]
async fn test_rename_link() {}

#[tokio::test]
async fn test_set_pinned_link() {}

#[tokio::test]
async fn test_update_link_value() {}

#[tokio::test]
async fn test_all_links() {}

#[tokio::test]
async fn test_find_link() {}

#[tokio::test]
async fn test_resolve_link_path() {
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

    let root_link = Link::create(
        &db,
        CreateLink {
            name: "Root".to_string(),
            value: "http://test.com".to_string(),
            folder_id: root.id,
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

    let nested_link = Link::create(
        &db,
        CreateLink {
            name: "Root".to_string(),
            value: "http://test.com".to_string(),
            folder_id: base_folder.id,
            created_by: None,
        },
    )
    .await
    .unwrap();

    let nested_path = Link::resolve_path(&db, root_link.id).await.unwrap();

    assert_eq!(nested_path.len(), 1);

    assert_eq!(nested_path[0].id, root.id);
    assert_eq!(nested_path[0].name, root.name);

    let nested_path = Link::resolve_path(&db, nested_link.id).await.unwrap();

    assert_eq!(nested_path.len(), 2);

    assert_eq!(nested_path[0].id, root.id);
    assert_eq!(nested_path[0].name, root.name);

    assert_eq!(nested_path[1].id, base_folder.id);
    assert_eq!(nested_path[1].name, base_folder.name);
}

#[tokio::test]
async fn test_find_links_by_parent() {}

#[tokio::test]
async fn test_delete_link() {}

#[tokio::test]
async fn test_resolve_links_with_extra_mixed_scopes() {}

#[tokio::test]
async fn test_resolve_with_extra_link() {}

#[tokio::test]
async fn test_find_by_parent_with_extra_link() {}

#[tokio::test]
async fn test_find_with_extra_link() {}

#[tokio::test]
async fn test_total_count_link() {}
