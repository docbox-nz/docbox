use docbox_database::{models::user::User, utils::DatabaseErrorExt};
use uuid::Uuid;

use crate::common::{database::test_tenant_db, make_test_document_box, make_test_link};

mod common;

/// Tests that creating a user works correctly
#[tokio::test]
async fn test_create_user() {
    let (db, _db_container) = test_tenant_db().await;
    let id = Uuid::new_v4().to_string();
    let name = "test".to_string();
    let image_id = "test.png".to_string();

    let user = User::store(&db, id.clone(), Some(name.clone()), Some(image_id.clone()))
        .await
        .unwrap();

    assert_eq!(user.id, id);
    assert_eq!(user.name, Some(name.clone()));
    assert_eq!(user.image_id, Some(image_id.clone()));

    let user = User::find(&db, &id)
        .await
        .unwrap()
        .expect("expected user to be found");

    assert_eq!(user.id, id);
    assert_eq!(user.name, Some(name));
    assert_eq!(user.image_id, Some(image_id));
}

/// Tests that updating a user works correctly
#[tokio::test]
async fn test_update_user() {
    let (db, _db_container) = test_tenant_db().await;
    let id = Uuid::new_v4().to_string();
    let name = "test".to_string();
    let image_id = "test.png".to_string();

    let user = User::store(&db, id.clone(), Some(name.clone()), Some(image_id.clone()))
        .await
        .unwrap();

    assert_eq!(user.id, id);
    assert_eq!(user.name, Some(name));
    assert_eq!(user.image_id, Some(image_id));

    let name = "test2".to_string();
    let image_id = "test2.png".to_string();

    let user = User::store(&db, id.clone(), Some(name.clone()), Some(image_id.clone()))
        .await
        .unwrap();

    assert_eq!(user.id, id);
    assert_eq!(user.name, Some(name.clone()));
    assert_eq!(user.image_id, Some(image_id.clone()));

    let user = User::find(&db, &id)
        .await
        .unwrap()
        .expect("expected user to be found");

    assert_eq!(user.id, id);
    assert_eq!(user.name, Some(name));
    assert_eq!(user.image_id, Some(image_id));
}

/// Tests that a user can be queried up by ID
#[tokio::test]
async fn test_get_user() {
    let (db, _db_container) = test_tenant_db().await;
    let id = Uuid::new_v4().to_string();
    let name = "test".to_string();
    let image_id = "test.png".to_string();

    let created = User::store(&db, id.clone(), Some(name.clone()), Some(image_id.clone()))
        .await
        .unwrap();

    let user = User::find(&db, &id)
        .await
        .unwrap()
        .expect("expected user to be found");

    assert_eq!(user.id, created.id);
    assert_eq!(user.name, created.name);
    assert_eq!(user.image_id, created.image_id);
}

/// Tests that searching for an unknown user should return [None]
#[tokio::test]
async fn test_get_user_unknown() {
    let (db, _db_container) = test_tenant_db().await;

    let user = User::find(&db, "random").await.unwrap();
    assert!(user.is_none())
}

/// Tests that a collection of users can be queried
#[tokio::test]
async fn test_query_users() {
    let (db, _db_container) = test_tenant_db().await;

    let mut created_users = Vec::new();

    const ITEMS: usize = 100;
    const ITEMS_PER_PAGE: usize = 20;
    const PAGES: usize = ITEMS / ITEMS_PER_PAGE;

    for i in 0..ITEMS {
        let created = User::store(
            &db,
            format!("user-{i}"),
            Some(format!("user-{i}")),
            Some(format!("user-{i}")),
        )
        .await
        .unwrap();

        created_users.push(created);
    }

    created_users.sort_by_key(|value| value.id.clone());
    created_users.reverse();

    let users = User::query(&db, 0, ITEMS as u64).await.unwrap();
    assert_eq!(users, created_users);

    for i in 0..PAGES {
        let users = User::query(&db, (i * ITEMS_PER_PAGE) as u64, ITEMS_PER_PAGE as u64)
            .await
            .unwrap();

        assert_eq!(
            users,
            created_users
                .get((i * ITEMS_PER_PAGE)..((i + 1) * ITEMS_PER_PAGE))
                .unwrap()
        );
    }
}

/// Tests that the total users can be queries
#[tokio::test]
async fn test_query_total_users() {
    let (db, _db_container) = test_tenant_db().await;

    let mut created_users = Vec::new();

    const ITEMS: usize = 100;

    for i in 0..ITEMS {
        let created = User::store(
            &db,
            format!("user-{i}"),
            Some(format!("user-{i}")),
            Some(format!("user-{i}")),
        )
        .await
        .unwrap();

        created_users.push(created);
    }

    let users = User::total(&db).await.unwrap();
    assert_eq!(users, ITEMS as i64);
}

/// Tests that a user can be deleted
#[tokio::test]
async fn test_delete_user() {
    let (db, _db_container) = test_tenant_db().await;
    let id = Uuid::new_v4().to_string();
    let name = "test".to_string();
    let image_id = "test.png".to_string();

    let created = User::store(&db, id.clone(), Some(name.clone()), Some(image_id.clone()))
        .await
        .unwrap();
    let _created_2 = User::store(&db, "test-2".to_string(), None, None)
        .await
        .unwrap();

    let user = User::find(&db, &id)
        .await
        .unwrap()
        .expect("expected user to be found");

    assert_eq!(user, created);

    // Delete should affect one row
    let result = created.clone().delete(&db).await.unwrap();
    assert_eq!(result.rows_affected(), 1);

    // Attempting to delete again should have no affected rows
    let result = created.delete(&db).await.unwrap();
    assert_eq!(result.rows_affected(), 0);

    // Should not be able to find the user
    let user = User::find(&db, &id).await.unwrap();
    assert!(user.is_none());
}

/// Tests that a user cannot be deleted while its still referenced by resources
#[tokio::test]
async fn test_delete_user_restrict_when_owned() {
    let (db, _db_container) = test_tenant_db().await;
    let id = Uuid::new_v4().to_string();
    let name = "test".to_string();
    let image_id = "test.png".to_string();

    let created = User::store(&db, id.clone(), Some(name.clone()), Some(image_id.clone()))
        .await
        .unwrap();

    let (document_box, root) =
        make_test_document_box(&db, "test_1", Some(created.id.clone())).await;
    let base_link = make_test_link(&db, &root, "base", Some(created.id.clone())).await;

    // Should not be able to delete the user while it still has resources associated
    let error = created.clone().delete(&db).await.unwrap_err();
    assert!(error.is_restrict());

    // Delete the resources
    base_link.delete(&db).await.unwrap();
    root.delete(&db).await.unwrap();
    document_box.delete(&db).await.unwrap();

    // Delete should affect one row
    let result = created.clone().delete(&db).await.unwrap();
    assert_eq!(result.rows_affected(), 1);
}
