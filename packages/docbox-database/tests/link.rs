use crate::common::{
    database::test_tenant_db, make_test_document_box, make_test_folder, make_test_link,
    make_test_user,
};
use docbox_database::models::{
    link::{CreateLink, Link},
    shared::{DocboxInputPair, FolderPathSegment},
};
use uuid::Uuid;

mod common;

/// Tests that a link can be created
#[tokio::test]
async fn test_link_create() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, root) = make_test_document_box(&db, "test", None).await;

    let link = Link::create(
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

    assert_eq!(link.name, "Root");
    assert_eq!(link.value, "http://test.com");
    assert_eq!(link.folder_id, root.id);
    assert_eq!(link.created_by, None);

    let result = Link::find(&db, &document_box.scope, link.id)
        .await
        .unwrap()
        .expect("link should exist");
    assert_eq!(result, link);
}

/// Tests that a link can be moved to another folder
#[tokio::test]
async fn test_link_move_to_folder() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, root) = make_test_document_box(&db, "test_1", None).await;

    let base_link = make_test_link(&db, &root, "base", None).await;
    let base_folder = make_test_folder(&db, &root, "base_2", None).await;

    assert_eq!(base_link.folder_id, root.id);

    let base_result = Link::find(&db, &document_box.scope, base_link.id)
        .await
        .unwrap()
        .expect("folder should exist");

    assert_eq!(base_result, base_link);

    let base_link = base_link.move_to_folder(&db, base_folder.id).await.unwrap();

    // Change should be applied to the returned value
    assert_eq!(base_link.folder_id, base_folder.id);

    // Change should also apply to find results
    let base_result = Link::find(&db, &document_box.scope, base_link.id)
        .await
        .unwrap()
        .expect("folder should exist");
    assert_eq!(base_result.folder_id, base_folder.id);
}

/// Tests that a link can be renamed
#[tokio::test]
async fn test_link_rename() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, root) = make_test_document_box(&db, "test_1", None).await;

    let base_link = make_test_link(&db, &root, "base", None).await;
    assert_eq!(base_link.name, "base");

    let base_result = Link::find(&db, &document_box.scope, base_link.id)
        .await
        .unwrap()
        .expect("folder should exist");

    assert_eq!(base_result, base_link);

    let base_link = base_link.rename(&db, "base_2".to_string()).await.unwrap();

    // Change should be applied to the returned value
    assert_eq!(base_link.name, "base_2");

    // Change should also apply to find results
    let base_result = Link::find(&db, &document_box.scope, base_link.id)
        .await
        .unwrap()
        .expect("folder should exist");
    assert_eq!(base_result.name, "base_2");
}

/// Tests that a link can be pinned and unpinned
#[tokio::test]
async fn test_link_set_pinned() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, root) = make_test_document_box(&db, "test_1", None).await;

    let base_link = make_test_link(&db, &root, "base", None).await;
    assert!(!base_link.pinned);

    let base_result = Link::find(&db, &document_box.scope, base_link.id)
        .await
        .unwrap()
        .expect("folder should exist");

    assert_eq!(base_result, base_link);

    let base_link = base_link.set_pinned(&db, true).await.unwrap();

    // Change should be applied to the returned value
    assert!(base_link.pinned);

    // Change should also apply to find results
    let base_result = Link::find(&db, &document_box.scope, base_link.id)
        .await
        .unwrap()
        .expect("folder should exist");
    assert!(base_result.pinned);

    let base_folder = base_link.set_pinned(&db, false).await.unwrap();

    // Change should be applied to the returned value
    assert!(!base_folder.pinned);

    // Change should also apply to find results
    let base_result = Link::find(&db, &document_box.scope, base_folder.id)
        .await
        .unwrap()
        .expect("folder should exist");
    assert!(!base_result.pinned);
}

/// Tests that a link value can be updated
#[tokio::test]
async fn test_link_update_value() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, root) = make_test_document_box(&db, "test_1", None).await;

    let base_link = Link::create(
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

    assert_eq!(base_link.value, "http://test.com");

    let base_result = Link::find(&db, &document_box.scope, base_link.id)
        .await
        .unwrap()
        .expect("folder should exist");

    assert_eq!(base_result, base_link);

    let base_link = base_link
        .update_value(&db, "http://123".to_string())
        .await
        .unwrap();

    // Change should be applied to the returned value
    assert_eq!(base_link.value, "http://123");

    // Change should also apply to find results
    let base_result = Link::find(&db, &document_box.scope, base_link.id)
        .await
        .unwrap()
        .expect("folder should exist");
    assert_eq!(base_result.value, "http://123");
}

/// Tests that all links can be queried
#[tokio::test]
async fn test_link_all_links() {
    let (db, _db_container) = test_tenant_db().await;
    let (_document_box, root) = make_test_document_box(&db, "test_1", None).await;

    let links = Link::all(&db, 0, 5).await.unwrap();
    assert!(links.is_empty());

    let link_1 = make_test_link(&db, &root, "Test 1", None).await;
    let link_2 = make_test_link(&db, &root, "Test 2", None).await;
    let link_3 = make_test_link(&db, &root, "Test 3", None).await;

    let links = Link::all(&db, 0, 5).await.unwrap();
    assert_eq!(links.len(), 3);
    assert!(links.iter().any(|item| item.link.id == link_1.id));
    assert!(links.iter().any(|item| item.link.id == link_2.id));
    assert!(links.iter().any(|item| item.link.id == link_3.id));
}

/// Tests that links can be found by ID
#[tokio::test]
async fn test_link_find() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, root) = make_test_document_box(&db, "test", None).await;

    let link = make_test_link(&db, &root, "Test Link", None).await;
    let result = Link::find(&db, &document_box.scope, link.id)
        .await
        .unwrap()
        .expect("link should exist");
    assert_eq!(result, link);

    let result = Link::find(&db, &document_box.scope, Uuid::nil())
        .await
        .unwrap();
    assert!(result.is_none());
}

/// Tests that a link path can be resolved
#[tokio::test]
async fn test_link_resolve_path() {
    let (db, _db_container) = test_tenant_db().await;
    let (_document_box, root) = make_test_document_box(&db, "test", None).await;

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

    let base_folder = make_test_folder(&db, &root, "base", None).await;
    let nested_link = make_test_link(&db, &base_folder, "Test Link", None).await;

    let nested_path = Link::resolve_path(&db, root_link.id).await.unwrap();
    assert_eq!(&nested_path, &[FolderPathSegment::from(&root)]);

    let nested_path = Link::resolve_path(&db, nested_link.id).await.unwrap();
    assert_eq!(
        &nested_path,
        &[
            FolderPathSegment::from(&root),
            FolderPathSegment::from(&base_folder)
        ]
    );
}

/// Tests that links can be found by the parent folder
#[tokio::test]
async fn test_link_find_by_parent() {
    let (db, _db_container) = test_tenant_db().await;
    let (_document_box, root) = make_test_document_box(&db, "test_1", None).await;

    let links = Link::find_by_parent(&db, root.id).await.unwrap();
    assert!(links.is_empty());

    let link_1 = make_test_link(&db, &root, "Test 1", None).await;
    let link_2 = make_test_link(&db, &root, "Test 2", None).await;
    let link_3 = make_test_link(&db, &root, "Test 3", None).await;

    let links = Link::find_by_parent(&db, root.id).await.unwrap();
    assert_eq!(links.len(), 3);
    assert!(links.iter().any(|item| item.id == link_1.id));
    assert!(links.iter().any(|item| item.id == link_2.id));
    assert!(links.iter().any(|item| item.id == link_3.id));

    let base_folder_1 = make_test_folder(&db, &root, "base_1", None).await;
    let links = Link::find_by_parent(&db, base_folder_1.id).await.unwrap();
    assert!(links.is_empty());

    let link_4 = make_test_link(&db, &base_folder_1, "Test 4", None).await;
    let link_5 = make_test_link(&db, &base_folder_1, "Test 5", None).await;
    let link_6 = make_test_link(&db, &base_folder_1, "Test 6", None).await;

    let links = Link::find_by_parent(&db, base_folder_1.id).await.unwrap();
    assert_eq!(links.len(), 3);
    assert!(links.iter().any(|item| item.id == link_4.id));
    assert!(links.iter().any(|item| item.id == link_5.id));
    assert!(links.iter().any(|item| item.id == link_6.id));
}

/// Tests that links can be deleted
#[tokio::test]
async fn test_link_delete() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, root) = make_test_document_box(&db, "test_1", None).await;

    let link = make_test_link(&db, &root, "test", None).await;
    let other_link = make_test_link(&db, &root, "test_2", None).await;

    // Link should exist
    let target = Link::find(&db, &document_box.scope, link.id).await.unwrap();
    assert!(target.is_some());

    // Delete link should delete one row
    let result = link.delete(&db).await.unwrap();
    assert_eq!(result.rows_affected(), 1);

    // Link shouldn't exist
    let target = Link::find(&db, &document_box.scope, link.id).await.unwrap();
    assert!(target.is_none());

    // Other link should still exist
    let target = Link::find(&db, &document_box.scope, other_link.id)
        .await
        .unwrap();
    assert!(target.is_some());

    // Delete link shouldn't delete any rows now that its gone
    let result = link.delete(&db).await.unwrap();
    assert_eq!(result.rows_affected(), 0);

    // Should not be able to delete the root while another link is still present
    // (Enforce proper deletion)
    let result = root.delete(&db).await.unwrap_err();
    assert_eq!(
        result.into_database_error().unwrap().code().unwrap(),
        // RESTRICT foreign key constraint violation
        "23001"
    );
}

/// Tests that links can be resolved by a collection of scope and link ID pairs
#[tokio::test]
async fn test_link_resolve_with_extra_mixed_scopes() {
    let (db, _db_container) = test_tenant_db().await;

    let (document_box_1, root_1) = make_test_document_box(&db, "test_1", None).await;
    let (document_box_2, root_2) = make_test_document_box(&db, "test_2", None).await;

    let nested = make_test_folder(&db, &root_2, "Nested", None).await;
    let link_1 = make_test_link(&db, &root_1, "Link 1", None).await;
    let user = make_test_user(&db, "Test User").await;
    let link_2 = make_test_link(&db, &root_2, "Link 2", Some(user.id.clone())).await;

    let link_3 = make_test_link(&db, &nested, "Link 3", None).await;
    let link_4 = make_test_link(&db, &nested, "Link 4", Some(user.id.clone())).await;

    let links = Link::resolve_with_extra_mixed_scopes(
        &db,
        vec![
            DocboxInputPair::new(&document_box_1.scope, link_1.id),
            DocboxInputPair::new(&document_box_2.scope, link_2.id),
            DocboxInputPair::new(&document_box_2.scope, link_3.id),
            DocboxInputPair::new(&document_box_2.scope, link_4.id),
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
    assert_eq!(link.document_box, document_box_1.scope);
    assert_eq!(&link.full_path, &[FolderPathSegment::from(&root_1),]);

    // Should locate and lookup the created_by user for the second link
    let link = links
        .iter()
        .find(|link| link.data.link.id == link_2.id)
        .expect("link should exist");

    assert_eq!(link.data.link, link_2);
    assert_eq!(link.data.last_modified_at, None);
    assert_eq!(link.data.last_modified_by, None);
    assert_eq!(link.data.created_by, Some(user.clone()));
    assert_eq!(link.document_box, document_box_2.scope);
    assert_eq!(&link.full_path, &[FolderPathSegment::from(&root_2),]);

    // Should locate the third link and resolve the nested path
    let link = links
        .iter()
        .find(|link| link.data.link.id == link_3.id)
        .expect("link should exist");

    assert_eq!(link.data.link, link_3);
    assert_eq!(link.data.last_modified_at, None);
    assert_eq!(link.data.last_modified_by, None);
    assert_eq!(link.data.created_by, None);
    assert_eq!(link.document_box, document_box_2.scope);
    assert_eq!(
        &link.full_path,
        &[
            FolderPathSegment::from(&root_2),
            FolderPathSegment::from(&nested)
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
    assert_eq!(link.document_box, document_box_2.scope);
    assert_eq!(
        &link.full_path,
        &[
            FolderPathSegment::from(&root_2),
            FolderPathSegment::from(&nested)
        ]
    );
}

/// Tests that a collection of links can be resolved by ID within a single scope
#[tokio::test]
async fn test_link_resolve_with_extra() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, root) = make_test_document_box(&db, "test_1", None).await;

    let nested = make_test_folder(&db, &root, "Nested", None).await;

    let link_1 = make_test_link(&db, &root, "Link 1", None).await;
    let user = make_test_user(&db, "Test User").await;
    let link_2 = make_test_link(&db, &root, "Link 2", Some(user.id.clone())).await;
    let link_3 = make_test_link(&db, &nested, "Link 3", None).await;
    let link_4 = make_test_link(&db, &nested, "Link 4", Some(user.id.clone())).await;

    let links = Link::resolve_with_extra(
        &db,
        &document_box.scope,
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
    assert_eq!(&link.full_path, &[FolderPathSegment::from(&root)]);

    // Should locate and lookup the created_by user for the second link
    let link = links
        .iter()
        .find(|link| link.data.link.id == link_2.id)
        .expect("link should exist");

    assert_eq!(link.data.link, link_2);
    assert_eq!(link.data.last_modified_at, None);
    assert_eq!(link.data.last_modified_by, None);
    assert_eq!(link.data.created_by, Some(user.clone()));
    assert_eq!(&link.full_path, &[FolderPathSegment::from(&root)]);

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
        &link.full_path,
        &[
            FolderPathSegment::from(&root),
            FolderPathSegment::from(&nested)
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
        &link.full_path,
        &[
            FolderPathSegment::from(&root),
            FolderPathSegment::from(&nested)
        ]
    );
}

/// Tests that links can be resolved by the parent with extra data
#[tokio::test]
async fn test_link_find_by_parent_with_extra() {
    let (db, _db_container) = test_tenant_db().await;
    let (_document_box, root) = make_test_document_box(&db, "test_1", None).await;

    let link_1 = make_test_link(&db, &root, "Link 1", None).await;
    let user = make_test_user(&db, "Test User").await;
    let link_2 = make_test_link(&db, &root, "Link 2", Some(user.id.clone())).await;

    let nested = make_test_folder(&db, &root, "Nested", None).await;

    let links = Link::find_by_parent_with_extra(&db, nested.id)
        .await
        .unwrap();

    assert!(links.is_empty());

    let link_3 = make_test_link(&db, &nested, "Link 3", None).await;
    let link_4 = make_test_link(&db, &nested, "Link 4", Some(user.id.clone())).await;

    let links = Link::find_by_parent_with_extra(&db, root.id).await.unwrap();

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

/// Tests that links can be found by ID with extra
#[tokio::test]
async fn test_link_find_with_extra() {
    let (db, _db_container) = test_tenant_db().await;

    let (scope_1, root_1) = make_test_document_box(&db, "test_1", None).await;
    let (scope_2, root_2) = make_test_document_box(&db, "test_2", None).await;

    let link_1 = make_test_link(&db, &root_1, "Link 1", None).await;

    let user = make_test_user(&db, "Test User").await;
    let link_2 = make_test_link(&db, &root_2, "Link 2", Some(user.id.clone())).await;

    // Should be able to find the link in the first scope
    let link = Link::find_with_extra(&db, &scope_1.scope, link_1.id)
        .await
        .unwrap()
        .expect("link should exist");

    assert_eq!(link.link, link_1);
    assert_eq!(link.last_modified_at, None);
    assert_eq!(link.last_modified_by, None);
    assert_eq!(link.created_by, None);

    // Searching for the unknown link in the second scope should result in nothing
    let link = Link::find_with_extra(&db, &scope_2.scope, link_1.id)
        .await
        .unwrap();
    assert!(link.is_none());

    // Should locate and lookup the created_by user for the second link
    let link = Link::find_with_extra(&db, &scope_2.scope, link_2.id)
        .await
        .unwrap()
        .expect("link should exist");

    assert_eq!(link.link, link_2);
    assert_eq!(link.last_modified_at, None);
    assert_eq!(link.last_modified_by, None);
    assert_eq!(link.created_by, Some(user.clone()));
}

/// Tests that the link count can be retrieved
#[tokio::test]
async fn test_link_total_count() {
    let (db, _db_container) = test_tenant_db().await;
    let (_document_box, root) = make_test_document_box(&db, "test_1", None).await;

    let count = Link::total_count(&db).await.unwrap();
    assert_eq!(count, 0);

    const LINK_COUNT: i64 = 15;

    for i in 0..LINK_COUNT {
        make_test_link(&db, &root, format!("Test {i}"), None).await;
    }

    let count = Link::total_count(&db).await.unwrap();
    assert_eq!(count, LINK_COUNT);
}
