use docbox_database::{
    models::tenant::{CreateTenant, Tenant},
    utils::DatabaseErrorExt,
};
use uuid::Uuid;

use crate::common::database::test_root_db;

mod common;

/// Tests that a tenant can be created
#[tokio::test]
async fn test_create_tenant() {
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
}

/// Tests that duplicate tenant fields cannot be violated
#[tokio::test]
async fn test_create_tenant_duplicate() {
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

    // Test duplicate ID in the same env
    let error = Tenant::create(
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
    .unwrap_err();
    assert!(error.is_duplicate_record());

    // Test duplicate db name
    let error = Tenant::create(
        &db,
        CreateTenant {
            id: Uuid::new_v4(),
            name: "test".to_string(),
            db_name: "test".to_string(),
            db_secret_name: Some("test".to_string()),
            db_iam_user_name: None,
            s3_name: "test-dev".to_string(),
            os_index_name: "test-dev".to_string(),
            event_queue_url: None,
            env: "Production".to_string(),
        },
    )
    .await
    .unwrap_err();
    assert!(error.is_duplicate_record());

    // Test duplicate db secret name
    let error = Tenant::create(
        &db,
        CreateTenant {
            id: Uuid::new_v4(),
            name: "test".to_string(),
            db_name: "test-dev".to_string(),
            db_secret_name: Some("test".to_string()),
            db_iam_user_name: None,
            s3_name: "test-dev".to_string(),
            os_index_name: "test-dev".to_string(),
            event_queue_url: None,
            env: "Production".to_string(),
        },
    )
    .await
    .unwrap_err();
    assert!(error.is_duplicate_record());

    // Test duplicate storage bucket name
    let error = Tenant::create(
        &db,
        CreateTenant {
            id: Uuid::new_v4(),
            name: "test".to_string(),
            db_name: "test-dev".to_string(),
            db_secret_name: Some("test".to_string()),
            db_iam_user_name: None,
            s3_name: "test".to_string(),
            os_index_name: "test-dev".to_string(),
            event_queue_url: None,
            env: "Production".to_string(),
        },
    )
    .await
    .unwrap_err();
    assert!(error.is_duplicate_record());

    // Test duplicate search index name
    let error = Tenant::create(
        &db,
        CreateTenant {
            id: Uuid::new_v4(),
            name: "test".to_string(),
            db_name: "test-dev".to_string(),

            db_secret_name: Some("test".to_string()),
            db_iam_user_name: None,
            s3_name: "test-dev".to_string(),
            os_index_name: "test".to_string(),
            event_queue_url: None,
            env: "Production".to_string(),
        },
    )
    .await
    .unwrap_err();
    assert!(error.is_duplicate_record());
}

/// Tests that tenants can be found by ID
#[tokio::test]
async fn test_find_by_id_with_env() {
    let (db, _db_container) = test_root_db().await;

    let tenant_id = Uuid::new_v4();

    let created = Tenant::create(
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

    let tenant = Tenant::find_by_id(&db, tenant_id, "Development")
        .await
        .unwrap()
        .expect("tenant should exist");

    assert_eq!(tenant.name, created.name);
    assert_eq!(tenant.db_name, created.db_name);
    assert_eq!(tenant.db_secret_name, created.db_secret_name);
    assert_eq!(tenant.s3_name, created.s3_name);
    assert_eq!(tenant.os_index_name, created.os_index_name);
    assert_eq!(tenant.event_queue_url, created.event_queue_url);
    assert_eq!(tenant.env, created.env);

    // Test with unknown ID should find nothing
    let result = Tenant::find_by_id(&db, Uuid::nil(), "Development")
        .await
        .unwrap();
    assert!(result.is_none());

    // Test with different env should find nothing
    let result = Tenant::find_by_id(&db, tenant_id, "Production")
        .await
        .unwrap();
    assert!(result.is_none());
}

/// Tests that a tenant can be found by its S3 bucket
#[tokio::test]
async fn test_find_tenant_by_bucket() {
    let (db, _db_container) = test_root_db().await;

    let tenant_id = Uuid::new_v4();

    let created = Tenant::create(
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

    let tenant = Tenant::find_by_bucket(&db, "test")
        .await
        .unwrap()
        .expect("tenant should exist");

    assert_eq!(tenant.name, created.name);
    assert_eq!(tenant.db_name, created.db_name);
    assert_eq!(tenant.db_secret_name, created.db_secret_name);
    assert_eq!(tenant.s3_name, created.s3_name);
    assert_eq!(tenant.os_index_name, created.os_index_name);
    assert_eq!(tenant.event_queue_url, created.event_queue_url);
    assert_eq!(tenant.env, created.env);

    // Test with unknown bucket should find nothing
    let result = Tenant::find_by_bucket(&db, "unknown").await.unwrap();
    assert!(result.is_none());
}

/// Tests that tenants can be found by their env
#[tokio::test]
async fn test_find_tenant_by_env() {
    let (db, _db_container) = test_root_db().await;

    // Should be empty initially
    let tenants = Tenant::find_by_env(&db, "Development").await.unwrap();
    assert!(tenants.is_empty());

    let mut created = Vec::new();

    // Insert some dev tenants
    for i in 0..3 {
        let tenant = Tenant::create(
            &db,
            CreateTenant {
                id: Uuid::new_v4(),
                name: format!("test-{i}"),
                db_name: format!("test-{i}"),
                db_secret_name: Some(format!("test-{i}")),
                db_iam_user_name: None,
                s3_name: format!("test-{i}"),
                os_index_name: format!("test-{i}"),
                event_queue_url: None,
                env: "Development".to_string(),
            },
        )
        .await
        .unwrap();
        created.push(tenant);
    }

    // The created tenants should exist
    let tenants = Tenant::find_by_env(&db, "Development").await.unwrap();
    assert_eq!(tenants.len(), created.len());
    assert_eq!(tenants, created);

    // Different env should be empty
    let tenants = Tenant::find_by_env(&db, "Production").await.unwrap();
    assert!(tenants.is_empty());

    let mut created_prod = Vec::new();

    // Insert some prod tenants
    for i in 0..3 {
        let tenant = Tenant::create(
            &db,
            CreateTenant {
                id: Uuid::new_v4(),
                name: format!("test-{i}-prod"),
                db_name: format!("test-{i}-prod"),
                db_secret_name: Some(format!("test-{i}-prod")),
                db_iam_user_name: None,
                s3_name: format!("test-{i}-prod"),
                os_index_name: format!("test-{i}-prod"),
                event_queue_url: None,
                env: "Production".to_string(),
            },
        )
        .await
        .unwrap();
        created_prod.push(tenant);
    }

    // The created dev tenants should stay the same
    let tenants = Tenant::find_by_env(&db, "Development").await.unwrap();
    assert_eq!(tenants.len(), created.len());
    assert_eq!(tenants, created);

    // Prod should have tenants now
    let tenants = Tenant::find_by_env(&db, "Production").await.unwrap();
    assert_eq!(tenants.len(), created_prod.len());
    assert_eq!(tenants, created_prod);
}

#[tokio::test]
async fn test_all_tenants() {
    let (db, _db_container) = test_root_db().await;

    // Should be empty initially
    let tenants = Tenant::all(&db).await.unwrap();
    assert!(tenants.is_empty());

    let mut created = Vec::new();

    // Insert some dev tenants
    for i in 0..6 {
        let tenant = Tenant::create(
            &db,
            CreateTenant {
                id: Uuid::new_v4(),
                name: format!("test-{i}"),
                db_name: format!("test-{i}"),
                db_secret_name: Some(format!("test-{i}")),
                db_iam_user_name: None,
                s3_name: format!("test-{i}"),
                os_index_name: format!("test-{i}"),
                event_queue_url: None,
                env: "Development".to_string(),
            },
        )
        .await
        .unwrap();
        created.push(tenant);
    }

    // The created tenants should exist
    let tenants = Tenant::all(&db).await.unwrap();
    assert_eq!(tenants.len(), created.len());
    assert_eq!(tenants, created);
}

/// Tests that a tenant can be deleted
#[tokio::test]
async fn test_delete_tenant() {
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

    let second_tenant = Tenant::create(
        &db,
        CreateTenant {
            id: Uuid::new_v4(),
            name: "test-2".to_string(),
            db_name: "test-2".to_string(),
            db_secret_name: Some("test-2".to_string()),
            db_iam_user_name: None,
            s3_name: "test-2".to_string(),
            os_index_name: "test-2".to_string(),
            event_queue_url: None,
            env: "Development".to_string(),
        },
    )
    .await
    .unwrap();

    let tenant = Tenant::find_by_id(&db, tenant_id, "Development")
        .await
        .unwrap()
        .expect("tenant should exist");

    tenant.delete(&db).await.unwrap();

    // Tenant should no longer exist
    let result = Tenant::find_by_id(&db, tenant_id, "Development")
        .await
        .unwrap();
    assert!(result.is_none());

    // The other tenant should still exist
    let result = Tenant::find_by_id(&db, second_tenant.id, "Development")
        .await
        .unwrap();
    assert!(result.is_some());
}
