use docbox_database::models::{
    document_box::DocumentBox,
    folder::{CreateFolder, Folder, FolderPathSegment},
    link::{CreateLink, Link},
    shared::DocboxInputPair,
    user::User,
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
async fn test_resolve_links_with_extra_mixed_scopes() {
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

    let link_1 = Link::create(
        &db,
        CreateLink {
            name: "Link 1".to_string(),
            value: Default::default(),
            folder_id: root_1.id,
            created_by: None,
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

    let link_2 = Link::create(
        &db,
        CreateLink {
            name: "Link 2".to_string(),
            value: Default::default(),
            folder_id: root_2.id,
            created_by: Some(user.id.clone()),
        },
    )
    .await
    .unwrap();

    let link_3 = Link::create(
        &db,
        CreateLink {
            name: "Link 3".to_string(),
            value: Default::default(),
            folder_id: nested.id,
            created_by: None,
        },
    )
    .await
    .unwrap();

    let link_4 = Link::create(
        &db,
        CreateLink {
            name: "Link 4".to_string(),
            value: Default::default(),
            folder_id: nested.id,
            created_by: Some(user.id.clone()),
        },
    )
    .await
    .unwrap();

    let links = Link::resolve_with_extra_mixed_scopes(
        &db,
        vec![
            DocboxInputPair::new(&scope_1, link_1.id),
            DocboxInputPair::new(&scope_2, link_2.id),
            DocboxInputPair::new(&scope_2, link_3.id),
            DocboxInputPair::new(&scope_2, link_4.id),
        ],
    )
    .await
    .unwrap();

    // Should be able to find the link in the first scope
    let link = links
        .iter()
        .find(|link| link.data.link.id == link_1.id)
        .expect("link should exist");

    assert_eq!(link.data.link, link_1);
    assert_eq!(link.data.last_modified_at, None);
    assert_eq!(link.data.last_modified_by, None);
    assert_eq!(link.data.created_by, None);
    assert_eq!(link.document_box, scope_1);
    assert_eq!(
        link.full_path,
        vec![FolderPathSegment {
            id: root_1.id,
            name: root_1.name.clone(),
        }]
    );

    // Should locate and lookup the created_by user for the second link
    let link = links
        .iter()
        .find(|link| link.data.link.id == link_2.id)
        .expect("link should exist");

    assert_eq!(link.data.link, link_2);
    assert_eq!(link.data.last_modified_at, None);
    assert_eq!(link.data.last_modified_by, None);
    assert_eq!(link.data.created_by, Some(user.clone()));
    assert_eq!(link.document_box, scope_2);
    assert_eq!(
        link.full_path,
        vec![FolderPathSegment {
            id: root_2.id,
            name: root_2.name.clone(),
        }]
    );

    // Should locate the third link and resolve the nested path
    let link = links
        .iter()
        .find(|link| link.data.link.id == link_3.id)
        .expect("link should exist");

    assert_eq!(link.data.link, link_3);
    assert_eq!(link.data.last_modified_at, None);
    assert_eq!(link.data.last_modified_by, None);
    assert_eq!(link.data.created_by, None);
    assert_eq!(link.document_box, scope_2);
    assert_eq!(
        link.full_path,
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

    // Should locate the third link and resolve the nested path and the creator
    let link = links
        .iter()
        .find(|link| link.data.link.id == link_4.id)
        .expect("link should exist");

    assert_eq!(link.data.link, link_4);
    assert_eq!(link.data.last_modified_at, None);
    assert_eq!(link.data.last_modified_by, None);
    assert_eq!(link.data.created_by, Some(user.clone()));
    assert_eq!(link.document_box, scope_2);
    assert_eq!(
        link.full_path,
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

#[tokio::test]
async fn test_resolve_with_extra_link() {
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

    let link_1 = Link::create(
        &db,
        CreateLink {
            name: "Link 1".to_string(),
            value: Default::default(),
            folder_id: root_1.id,
            created_by: None,
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

    let link_2 = Link::create(
        &db,
        CreateLink {
            name: "Link 2".to_string(),
            value: Default::default(),
            folder_id: root_2.id,
            created_by: Some(user.id.clone()),
        },
    )
    .await
    .unwrap();

    let link_3 = Link::create(
        &db,
        CreateLink {
            name: "Link 3".to_string(),
            value: Default::default(),
            folder_id: nested.id,
            created_by: None,
        },
    )
    .await
    .unwrap();

    let link_4 = Link::create(
        &db,
        CreateLink {
            name: "Link 4".to_string(),
            value: Default::default(),
            folder_id: nested.id,
            created_by: Some(user.id.clone()),
        },
    )
    .await
    .unwrap();

    let links = Link::resolve_with_extra(
        &db,
        &scope_1,
        vec![link_1.id, link_2.id, link_3.id, link_4.id],
    )
    .await
    .unwrap();

    // Should be able to find the link in the first scope
    let link = links
        .iter()
        .find(|link| link.data.link.id == link_1.id)
        .expect("link should exist");

    assert_eq!(link.data.link, link_1);
    assert_eq!(link.data.last_modified_at, None);
    assert_eq!(link.data.last_modified_by, None);
    assert_eq!(link.data.created_by, None);
    assert_eq!(
        link.full_path,
        vec![FolderPathSegment {
            id: root_1.id,
            name: root_1.name.clone(),
        }]
    );

    // Should locate and lookup the created_by user for the second link
    let link = links
        .iter()
        .find(|link| link.data.link.id == link_2.id)
        .expect("link should exist");

    assert_eq!(link.data.link, link_2);
    assert_eq!(link.data.last_modified_at, None);
    assert_eq!(link.data.last_modified_by, None);
    assert_eq!(link.data.created_by, Some(user.clone()));
    assert_eq!(
        link.full_path,
        vec![FolderPathSegment {
            id: root_2.id,
            name: root_2.name.clone(),
        }]
    );

    // Should locate the third link and resolve the nested path
    let link = links
        .iter()
        .find(|link| link.data.link.id == link_3.id)
        .expect("link should exist");

    assert_eq!(link.data.link, link_3);
    assert_eq!(link.data.last_modified_at, None);
    assert_eq!(link.data.last_modified_by, None);
    assert_eq!(link.data.created_by, None);
    assert_eq!(
        link.full_path,
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

    // Should locate the third link and resolve the nested path and the creator
    let link = links
        .iter()
        .find(|link| link.data.link.id == link_4.id)
        .expect("link should exist");

    assert_eq!(link.data.link, link_4);
    assert_eq!(link.data.last_modified_at, None);
    assert_eq!(link.data.last_modified_by, None);
    assert_eq!(link.data.created_by, Some(user.clone()));
    assert_eq!(
        link.full_path,
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

#[tokio::test]
async fn test_find_by_parent_with_extra_link() {
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

    let link_1 = Link::create(
        &db,
        CreateLink {
            name: "Link 1".to_string(),
            value: Default::default(),
            folder_id: root_1.id,
            created_by: None,
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

    let link_2 = Link::create(
        &db,
        CreateLink {
            name: "Link 2".to_string(),
            value: Default::default(),
            folder_id: root_1.id,
            created_by: Some(user.id.clone()),
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

    let links = Link::find_by_parent_with_extra(&db, nested.id)
        .await
        .unwrap();

    assert!(links.is_empty());

    let link_3 = Link::create(
        &db,
        CreateLink {
            name: "Link 3".to_string(),
            value: Default::default(),
            folder_id: nested.id,
            created_by: None,
        },
    )
    .await
    .unwrap();

    let link_4 = Link::create(
        &db,
        CreateLink {
            name: "Link 4".to_string(),
            value: Default::default(),
            folder_id: nested.id,
            created_by: Some(user.id.clone()),
        },
    )
    .await
    .unwrap();

    let links = Link::find_by_parent_with_extra(&db, root_1.id)
        .await
        .unwrap();

    let link = links
        .iter()
        .find(|link| link.link.id == link_1.id)
        .expect("link should exist");

    assert_eq!(link.link, link_1);
    assert_eq!(link.last_modified_at, None);
    assert_eq!(link.last_modified_by, None);
    assert_eq!(link.created_by, None);

    let link = links
        .iter()
        .find(|link| link.link.id == link_2.id)
        .expect("link should exist");

    assert_eq!(link.link, link_2);
    assert_eq!(link.last_modified_at, None);
    assert_eq!(link.last_modified_by, None);
    assert_eq!(link.created_by, Some(user.clone()));

    let links = Link::find_by_parent_with_extra(&db, nested.id)
        .await
        .unwrap();

    let link = links
        .iter()
        .find(|link| link.link.id == link_3.id)
        .expect("link should exist");

    assert_eq!(link.link, link_3);
    assert_eq!(link.last_modified_at, None);
    assert_eq!(link.last_modified_by, None);
    assert_eq!(link.created_by, None);

    let link = links
        .iter()
        .find(|link| link.link.id == link_4.id)
        .expect("link should exist");

    assert_eq!(link.link, link_4);
    assert_eq!(link.last_modified_at, None);
    assert_eq!(link.last_modified_by, None);
    assert_eq!(link.created_by, Some(user.clone()));
}

#[tokio::test]
async fn test_find_with_extra_link() {
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

    let link_1 = Link::create(
        &db,
        CreateLink {
            name: "Link 1".to_string(),
            value: Default::default(),
            folder_id: root_1.id,
            created_by: None,
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

    let link_2 = Link::create(
        &db,
        CreateLink {
            name: "Link 2".to_string(),
            value: Default::default(),
            folder_id: root_2.id,
            created_by: Some(user.id.clone()),
        },
    )
    .await
    .unwrap();

    // Should be able to find the link in the first scope
    let link = Link::find_with_extra(&db, &scope_1, link_1.id)
        .await
        .unwrap()
        .expect("link should exist");

    assert_eq!(link.link, link_1);
    assert_eq!(link.last_modified_at, None);
    assert_eq!(link.last_modified_by, None);
    assert_eq!(link.created_by, None);

    // Searching for the unknown link in the second scope should result in nothing
    let link = Link::find_with_extra(&db, &scope_2, link_1.id)
        .await
        .unwrap();
    assert!(link.is_none());

    // Should locate and lookup the created_by user for the second link
    let link = Link::find_with_extra(&db, &scope_2, link_2.id)
        .await
        .unwrap()
        .expect("link should exist");

    assert_eq!(link.link, link_2);
    assert_eq!(link.last_modified_at, None);
    assert_eq!(link.last_modified_by, None);
    assert_eq!(link.created_by, Some(user.clone()));
}

#[tokio::test]
async fn test_total_count_link() {}
