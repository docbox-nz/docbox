use chrono::Utc;
use docbox_database::{
    migrations::initialize_root_migrations,
    models::root_migration::{CreateRootMigration, RootMigration},
};

use crate::common::database::{test_database, test_database_container};

mod common;

/// Tests a root migration can be created
#[tokio::test]
async fn test_root_migration_create() {
    let db_container = test_database_container().await;
    let db = test_database(&db_container).await;
    initialize_root_migrations(&db).await.unwrap();

    RootMigration::create(
        &db,
        CreateRootMigration {
            name: "test".to_string(),
            applied_at: Utc::now(),
        },
    )
    .await
    .unwrap();

    let migrations = RootMigration::all(&db).await.unwrap();
    assert_eq!(migrations.len(), 1);
    let migration = migrations.first().unwrap();
    assert_eq!(migration.name, "test");
}

/// Tests that inserting a duplicate migration will result in a uniqueness error
#[tokio::test]
async fn test_root_migration_create_duplicate() {
    let db_container = test_database_container().await;
    let db = test_database(&db_container).await;
    initialize_root_migrations(&db).await.unwrap();

    RootMigration::create(
        &db,
        CreateRootMigration {
            name: "test".to_string(),
            applied_at: Utc::now(),
        },
    )
    .await
    .unwrap();

    let error = RootMigration::create(
        &db,
        CreateRootMigration {
            name: "test".to_string(),
            applied_at: Utc::now(),
        },
    )
    .await
    .unwrap_err();
    assert!(error.into_database_error().unwrap().is_unique_violation());
}

/// Tests that all root migrations can be retrieved
#[tokio::test]
async fn test_root_migration_all() {
    let db_container = test_database_container().await;
    let db = test_database(&db_container).await;
    initialize_root_migrations(&db).await.unwrap();

    let migrations = RootMigration::all(&db).await.unwrap();
    assert!(migrations.is_empty());

    RootMigration::create(
        &db,
        CreateRootMigration {
            name: "test".to_string(),
            applied_at: Utc::now(),
        },
    )
    .await
    .unwrap();

    let migrations = RootMigration::all(&db).await.unwrap();
    assert_eq!(migrations.len(), 1);
    let migration = migrations.first().unwrap();
    assert_eq!(migration.name, "test");

    RootMigration::create(
        &db,
        CreateRootMigration {
            name: "test_2".to_string(),
            applied_at: Utc::now(),
        },
    )
    .await
    .unwrap();

    let migrations = RootMigration::all(&db).await.unwrap();
    assert_eq!(migrations.len(), 2);
    let migration = migrations.first().unwrap();
    assert_eq!(migration.name, "test");
    let migration = migrations.get(1).unwrap();
    assert_eq!(migration.name, "test_2");
}
