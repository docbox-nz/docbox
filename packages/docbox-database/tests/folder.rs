use docbox_database::models::{
    document_box::DocumentBox,
    folder::{CreateFolder, Folder},
    link::{CreateLink, Link},
    shared::DocboxInputPair,
};
use tokio::time::Instant;

use crate::common::database::test_tenant_db;

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

#[tokio::test]
async fn test_find_root_folder() {}

#[tokio::test]
async fn test_create_folder() {}

#[tokio::test]
async fn test_delete_folder() {}

#[tokio::test]
async fn test_resolve_folder_with_extra_mixed_scopes() {
    let (db, _db_container) = test_tenant_db().await;

    let scope_1 = "test_1".to_string();
    let scope_2 = "test_2".to_string();
    _ = DocumentBox::create(&db, scope_1.clone()).await.unwrap();
    _ = DocumentBox::create(&db, scope_2.clone()).await.unwrap();

    let root = Folder::create(
        &db,
        CreateFolder {
            name: "Root".to_string(),
            document_box: scope_1.to_string(),
            folder_id: None,
            created_by: None,
        },
    )
    .await
    .unwrap();

    let root_2 = Folder::create(
        &db,
        CreateFolder {
            name: "Root".to_string(),
            document_box: scope_2.to_string(),
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
            document_box: scope_1.clone(),
            folder_id: Some(root.id),
            created_by: None,
        },
    )
    .await
    .unwrap();

    let base_folder_2 = Folder::create(
        &db,
        CreateFolder {
            name: "base_2".to_string(),
            document_box: scope_2.clone(),
            folder_id: Some(root_2.id),
            created_by: None,
        },
    )
    .await
    .unwrap();

    let nested_folder = Folder::create(
        &db,
        CreateFolder {
            name: "nested".to_string(),
            document_box: scope_1.clone(),
            folder_id: Some(base_folder.id),
            created_by: None,
        },
    )
    .await
    .unwrap();

    let nested_folder_2 = Folder::create(
        &db,
        CreateFolder {
            name: "nested_2".to_string(),
            document_box: scope_1.clone(),
            folder_id: Some(base_folder.id),
            created_by: None,
        },
    )
    .await
    .unwrap();

    let nested_folder_3 = Folder::create(
        &db,
        CreateFolder {
            name: "nested_3".to_string(),
            document_box: scope_2.clone(),
            folder_id: Some(base_folder_2.id),
            created_by: None,
        },
    )
    .await
    .unwrap();

    let start = Instant::now();

    let resolved = Folder::resolve_with_extra_mixed_scopes(
        &db,
        vec![
            DocboxInputPair::new(&scope_1, nested_folder.id),
            DocboxInputPair::new(&scope_1, nested_folder_2.id),
            DocboxInputPair::new(&scope_2, base_folder_2.id),
            DocboxInputPair::new(&scope_2, nested_folder_3.id),
        ],
    )
    .await
    .unwrap();

    let end = Instant::now();
    let elapsed = end - start;

    println!("elapsed = {}", elapsed.as_micros());

    let resolved_1 = resolved
        .iter()
        .find(|item| item.data.folder.id == nested_folder.id)
        .unwrap();
    let nested_path = &resolved_1.full_path;

    assert_eq!(nested_path.len(), 2);

    assert_eq!(nested_path[0].id, root.id);
    assert_eq!(nested_path[0].name, root.name);

    assert_eq!(nested_path[1].id, base_folder.id);
    assert_eq!(nested_path[1].name, base_folder.name);

    let resolved_2 = resolved
        .iter()
        .find(|item| item.data.folder.id == nested_folder_2.id)
        .unwrap();
    let nested_path = &resolved_2.full_path;

    assert_eq!(nested_path.len(), 2);

    assert_eq!(nested_path[0].id, root.id);
    assert_eq!(nested_path[0].name, root.name);

    assert_eq!(nested_path[1].id, base_folder.id);
    assert_eq!(nested_path[1].name, base_folder.name);

    let resolved_3 = resolved
        .iter()
        .find(|item| item.data.folder.id == base_folder_2.id)
        .unwrap();
    let nested_path = &resolved_3.full_path;

    assert_eq!(nested_path.len(), 1);

    assert_eq!(nested_path[0].id, root_2.id);
    assert_eq!(nested_path[0].name, root_2.name);

    let resolved_4 = resolved
        .iter()
        .find(|item| item.data.folder.id == nested_folder_3.id)
        .unwrap();
    let nested_path = &resolved_4.full_path;

    assert_eq!(nested_path.len(), 2);

    assert_eq!(nested_path[0].id, root_2.id);
    assert_eq!(nested_path[0].name, root_2.name);

    assert_eq!(nested_path[1].id, base_folder_2.id);
    assert_eq!(nested_path[1].name, base_folder_2.name);
}

#[tokio::test]
async fn test_resolve_with_extra_folder() {
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

    let base_folder_2 = Folder::create(
        &db,
        CreateFolder {
            name: "base_2".to_string(),
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

    let nested_folder_2 = Folder::create(
        &db,
        CreateFolder {
            name: "nested_2".to_string(),
            document_box: scope.clone(),
            folder_id: Some(base_folder.id),
            created_by: None,
        },
    )
    .await
    .unwrap();

    let nested_folder_3 = Folder::create(
        &db,
        CreateFolder {
            name: "nested_3".to_string(),
            document_box: scope.clone(),
            folder_id: Some(base_folder_2.id),
            created_by: None,
        },
    )
    .await
    .unwrap();

    let resolved = Folder::resolve_with_extra(
        &db,
        &scope,
        vec![
            nested_folder.id,
            nested_folder_2.id,
            base_folder_2.id,
            nested_folder_3.id,
        ],
    )
    .await
    .unwrap();

    let resolved_1 = resolved
        .iter()
        .find(|item| item.data.folder.id == nested_folder.id)
        .unwrap();
    let nested_path = &resolved_1.full_path;

    assert_eq!(nested_path.len(), 2);

    assert_eq!(nested_path[0].id, root.id);
    assert_eq!(nested_path[0].name, root.name);

    assert_eq!(nested_path[1].id, base_folder.id);
    assert_eq!(nested_path[1].name, base_folder.name);

    let resolved_2 = resolved
        .iter()
        .find(|item| item.data.folder.id == nested_folder_2.id)
        .unwrap();
    let nested_path = &resolved_2.full_path;

    assert_eq!(nested_path.len(), 2);

    assert_eq!(nested_path[0].id, root.id);
    assert_eq!(nested_path[0].name, root.name);

    assert_eq!(nested_path[1].id, base_folder.id);
    assert_eq!(nested_path[1].name, base_folder.name);

    let resolved_3 = resolved
        .iter()
        .find(|item| item.data.folder.id == base_folder_2.id)
        .unwrap();
    let nested_path = &resolved_3.full_path;

    assert_eq!(nested_path.len(), 1);

    assert_eq!(nested_path[0].id, root.id);
    assert_eq!(nested_path[0].name, root.name);

    let resolved_4 = resolved
        .iter()
        .find(|item| item.data.folder.id == nested_folder_3.id)
        .unwrap();
    let nested_path = &resolved_4.full_path;

    assert_eq!(nested_path.len(), 2);

    assert_eq!(nested_path[0].id, root.id);
    assert_eq!(nested_path[0].name, root.name);

    assert_eq!(nested_path[1].id, base_folder_2.id);
    assert_eq!(nested_path[1].name, base_folder_2.name);
}

#[tokio::test]
async fn test_resolve_by_id_with_extra_folder() {}

#[tokio::test]
async fn test_find_by_parent_with_extra_folder() {}

#[tokio::test]
async fn test_find_root_with_extra_folder() {}

#[tokio::test]
async fn test_total_count_folder() {}
