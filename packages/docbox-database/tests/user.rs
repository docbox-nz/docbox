use docbox_database::models::user::User;
use uuid::Uuid;

use crate::common::database::test_tenant_db;

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

    let user = User::find(&db, id.clone())
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

    let user = User::find(&db, id.clone())
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

    let user = User::find(&db, id.clone())
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

    let user = User::find(&db, "random".to_string()).await.unwrap();
    assert!(user.is_none())
}
