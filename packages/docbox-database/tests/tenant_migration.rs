use chrono::Utc;
use docbox_database::{
    models::{
        tenant::{CreateTenant, Tenant},
        tenant_migration::{CreateTenantMigration, TenantMigration},
    },
    utils::DatabaseErrorExt,
};
use uuid::Uuid;

use crate::common::database::test_root_db;

mod common;

/// Tests that we can create a tenant migration
#[tokio::test]
async fn test_create_tenant_migration() {
    let (db, _db_container) = test_root_db().await;

    let tenant_id = Uuid::new_v4();

    Tenant::create(
        &db,
        CreateTenant {
            id: tenant_id,
            name: "test".to_string(),
            db_name: "test".to_string(),
            db_secret_name: Some("test".to_string()),
            db_iam_user_name: None,
            s3_name: "test".to_string(),
            os_index_name: "test".to_string(),
            event_queue_url: None,
            env: "Development".to_string(),
        },
    )
    .await
    .unwrap();

    TenantMigration::create(
        &db,
        CreateTenantMigration {
            tenant_id,
            env: "Development".to_string(),
            name: "m1_tenant_migration".to_string(),
            applied_at: Utc::now(),
        },
    )
    .await
    .unwrap();
}

/// Tests that duplicates are correctly handled as errors
#[tokio::test]
async fn test_create_tenant_migration_duplicate() {
    let (db, _db_container) = test_root_db().await;

    let tenant_id = Uuid::new_v4();

    Tenant::create(
        &db,
        CreateTenant {
            id: tenant_id,
            name: "test-dev".to_string(),
            db_name: "test-dev".to_string(),
            db_secret_name: Some("test-dev".to_string()),
            db_iam_user_name: None,
            s3_name: "test-dev".to_string(),
            os_index_name: "test-dev".to_string(),
            event_queue_url: None,
            env: "Development".to_string(),
        },
    )
    .await
    .unwrap();

    Tenant::create(
        &db,
        CreateTenant {
            id: tenant_id,
            name: "test-prod".to_string(),
            db_name: "test-prod".to_string(),
            db_secret_name: Some("test-prod".to_string()),
            db_iam_user_name: None,
            s3_name: "test-prod".to_string(),
            os_index_name: "test-prod".to_string(),
            event_queue_url: None,
            env: "Production".to_string(),
        },
    )
    .await
    .unwrap();

    TenantMigration::create(
        &db,
        CreateTenantMigration {
            tenant_id,
            env: "Development".to_string(),
            name: "m1_tenant_migration".to_string(),
            applied_at: Utc::now(),
        },
    )
    .await
    .unwrap();

    // Should not be able to insert duplicate ID + env + name combo
    let err = TenantMigration::create(
        &db,
        CreateTenantMigration {
            tenant_id,
            env: "Development".to_string(),
            name: "m1_tenant_migration".to_string(),
            applied_at: Utc::now(),
        },
    )
    .await
    .unwrap_err();

    assert!(err.is_duplicate_record());

    // Should be allowed to insert duplicate id + name but with different env
    TenantMigration::create(
        &db,
        CreateTenantMigration {
            tenant_id,
            env: "Production".to_string(),
            name: "m1_tenant_migration".to_string(),
            applied_at: Utc::now(),
        },
    )
    .await
    .unwrap();

    // Different name is not a duplicate
    TenantMigration::create(
        &db,
        CreateTenantMigration {
            tenant_id,
            env: "Development".to_string(),
            name: "m2_tenant_migration".to_string(),
            applied_at: Utc::now(),
        },
    )
    .await
    .unwrap();
}

/// Tests that we can retrieve the migrations for a tenant and that the env and tenant ID
/// requirement is respected
#[tokio::test]
async fn test_find_by_tenant_tenant_migration() {
    let (db, _db_container) = test_root_db().await;

    let tenant_id = Uuid::new_v4();

    Tenant::create(
        &db,
        CreateTenant {
            id: tenant_id,
            name: "test-dev".to_string(),
            db_name: "test-dev".to_string(),
            db_secret_name: Some("test-dev".to_string()),
            db_iam_user_name: None,
            s3_name: "test-dev".to_string(),
            os_index_name: "test-dev".to_string(),
            event_queue_url: None,
            env: "Development".to_string(),
        },
    )
    .await
    .unwrap();

    TenantMigration::create(
        &db,
        CreateTenantMigration {
            tenant_id,
            env: "Development".to_string(),
            name: "m1_tenant_migration".to_string(),
            applied_at: Utc::now(),
        },
    )
    .await
    .unwrap();

    TenantMigration::create(
        &db,
        CreateTenantMigration {
            tenant_id,
            env: "Development".to_string(),
            name: "m2_tenant_migration".to_string(),
            applied_at: Utc::now(),
        },
    )
    .await
    .unwrap();

    TenantMigration::create(
        &db,
        CreateTenantMigration {
            tenant_id,
            env: "Development".to_string(),
            name: "m3_tenant_migration".to_string(),
            applied_at: Utc::now(),
        },
    )
    .await
    .unwrap();

    Tenant::create(
        &db,
        CreateTenant {
            id: tenant_id,
            name: "test-prod".to_string(),
            db_name: "test-prod".to_string(),
            db_secret_name: Some("test-prod".to_string()),
            db_iam_user_name: None,
            s3_name: "test-prod".to_string(),
            os_index_name: "test-prod".to_string(),
            event_queue_url: None,
            env: "Production".to_string(),
        },
    )
    .await
    .unwrap();

    TenantMigration::create(
        &db,
        CreateTenantMigration {
            tenant_id,
            env: "Production".to_string(),
            name: "m1_tenant_migration".to_string(),
            applied_at: Utc::now(),
        },
    )
    .await
    .unwrap();

    TenantMigration::create(
        &db,
        CreateTenantMigration {
            tenant_id,
            env: "Production".to_string(),
            name: "m2_tenant_migration".to_string(),
            applied_at: Utc::now(),
        },
    )
    .await
    .unwrap();

    TenantMigration::create(
        &db,
        CreateTenantMigration {
            tenant_id,
            env: "Production".to_string(),
            name: "m3_tenant_migration".to_string(),
            applied_at: Utc::now(),
        },
    )
    .await
    .unwrap();

    let migrations = TenantMigration::find_by_tenant(&db, tenant_id, "Development")
        .await
        .unwrap();

    assert_eq!(migrations.len(), 3);
    assert_eq!(migrations[0].name, "m1_tenant_migration");
    assert_eq!(migrations[1].name, "m2_tenant_migration");
    assert_eq!(migrations[2].name, "m3_tenant_migration");
}
