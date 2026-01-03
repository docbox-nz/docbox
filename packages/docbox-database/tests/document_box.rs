use docbox_database::{models::document_box::DocumentBox, utils::DatabaseErrorExt};

use crate::common::database::test_tenant_db;

mod common;

/// Tests that a document box can be created
#[tokio::test]
async fn test_create_document_box() {
    let (db, _db_container) = test_tenant_db().await;
    let document_box = DocumentBox::create(&db, "test".to_string()).await.unwrap();
    assert_eq!(document_box.scope.as_str(), "test");
}

/// Tests that the document box creation should fail when the scope is already
/// in use
#[tokio::test]
async fn test_create_document_box_duplicate_scope_failure() {
    let (db, _db_container) = test_tenant_db().await;
    let document_box = DocumentBox::create(&db, "test".to_string()).await.unwrap();
    assert_eq!(document_box.scope.as_str(), "test");
    let error = DocumentBox::create(&db, "test".to_string())
        .await
        .unwrap_err();
    assert!(error.is_duplicate_record());
}

/// Tests that deleting a known document box affects a single row
#[tokio::test]
async fn test_delete_document_box_known() {
    let (db, _db_container) = test_tenant_db().await;
    let document_box = DocumentBox::create(&db, "test".to_string()).await.unwrap();
    let result = document_box.delete(&db).await.unwrap();
    assert_eq!(result.rows_affected(), 1);
}

/// Tests that deleting a already deleted document box returns
/// zero affected rows
#[tokio::test]
async fn test_delete_document_box_deleted() {
    let (db, _db_container) = test_tenant_db().await;
    let document_box = DocumentBox::create(&db, "test".to_string()).await.unwrap();
    let result = document_box.delete(&db).await.unwrap();
    assert_eq!(result.rows_affected(), 1);
    let result = document_box.delete(&db).await.unwrap();
    assert_eq!(result.rows_affected(), 0);
}

/// Tests that deleting a document box only deletes the expected document box
#[tokio::test]
async fn test_delete_document_box_exact() {
    let (db, _db_container) = test_tenant_db().await;
    DocumentBox::create(&db, "test1".to_string()).await.unwrap();
    DocumentBox::create(&db, "test2".to_string()).await.unwrap();
    let document_box = DocumentBox::create(&db, "test".to_string()).await.unwrap();
    let result = document_box.delete(&db).await.unwrap();
    assert_eq!(result.rows_affected(), 1);
    let result = document_box.delete(&db).await.unwrap();
    assert_eq!(result.rows_affected(), 0);
}

/// Tests that a document box can be found when searching by scope
#[tokio::test]
async fn test_document_box_find_by_scope() {
    let (db, _db_container) = test_tenant_db().await;
    let document_box = DocumentBox::create(&db, "test".to_string()).await.unwrap();
    let result = DocumentBox::find_by_scope(&db, "test")
        .await
        .unwrap()
        .expect("should find document box");
    assert_eq!(result.scope, document_box.scope);
}

/// Tests that no document box should be found when search
/// for an unknown scope
#[tokio::test]
async fn test_document_box_find_by_scope_unknown() {
    let (db, _db_container) = test_tenant_db().await;
    DocumentBox::create(&db, "test1".to_string()).await.unwrap();
    DocumentBox::create(&db, "test2".to_string()).await.unwrap();
    let result = DocumentBox::find_by_scope(&db, "test").await.unwrap();
    assert!(result.is_none());
}

/// Tests that the total number of document boxes can be
/// correctly obtained
#[tokio::test]
async fn test_document_box_total() {
    let (db, _db_container) = test_tenant_db().await;

    // Initial total should be zero
    let total = DocumentBox::total(&db).await.unwrap();
    assert_eq!(total, 0);

    // Single creation should increase total by one
    let document_box = DocumentBox::create(&db, "test1".to_string()).await.unwrap();
    let total = DocumentBox::total(&db).await.unwrap();
    assert_eq!(total, 1);

    // Inserting results should increase total
    DocumentBox::create(&db, "test2".to_string()).await.unwrap();
    DocumentBox::create(&db, "test3".to_string()).await.unwrap();

    let total = DocumentBox::total(&db).await.unwrap();
    assert_eq!(total, 3);

    // Deleting results should decrease total
    _ = document_box.delete(&db).await.unwrap();
    let total = DocumentBox::total(&db).await.unwrap();
    assert_eq!(total, 2);
}

/// Tests that the list of document boxes can be queried
#[tokio::test]
async fn test_document_box_query() {
    let (db, _db_container) = test_tenant_db().await;

    // Initial query result should be empty
    let results = DocumentBox::query(&db, 0, 5).await.unwrap();
    assert!(results.is_empty());

    // After inserting we should get back the results newest first
    DocumentBox::create(&db, "test1".to_string()).await.unwrap();
    DocumentBox::create(&db, "test2".to_string()).await.unwrap();
    DocumentBox::create(&db, "test3".to_string()).await.unwrap();

    let results = DocumentBox::query(&db, 0, 5).await.unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].scope, "test3");
    assert_eq!(results[1].scope, "test2");
    assert_eq!(results[2].scope, "test1");

    // Pagination should work
    let results = DocumentBox::query(&db, 0, 2).await.unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].scope, "test3");
    assert_eq!(results[1].scope, "test2");
    let results = DocumentBox::query(&db, 2, 2).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].scope, "test1");

    let results = DocumentBox::query(&db, 3, 2).await.unwrap();
    assert!(results.is_empty());
}

/// Tests that a search query can be used to search for document boxes
#[tokio::test]
async fn test_document_box_search_query() {
    let (db, _db_container) = test_tenant_db().await;

    // Initial query result should be empty
    let results = DocumentBox::search_query(&db, "test", 0, 5).await.unwrap();
    assert!(results.is_empty());

    // Querying specific result
    DocumentBox::create(&db, "test1".to_string()).await.unwrap();
    DocumentBox::create(&db, "test2".to_string()).await.unwrap();
    DocumentBox::create(&db, "test3".to_string()).await.unwrap();

    let results = DocumentBox::search_query(&db, "test3", 0, 5).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].scope, "test3");

    // Querying wildcard results
    let results = DocumentBox::search_query(&db, "test%", 0, 5).await.unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].scope, "test3");
    assert_eq!(results[1].scope, "test2");
    assert_eq!(results[2].scope, "test1");

    // Querying wildcard results offset
    let results = DocumentBox::search_query(&db, "test%", 1, 5).await.unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].scope, "test2");
    assert_eq!(results[1].scope, "test1");

    // Querying wildcard results paginated
    let results = DocumentBox::search_query(&db, "test%", 0, 2).await.unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].scope, "test3");
    assert_eq!(results[1].scope, "test2");

    let results = DocumentBox::search_query(&db, "test%", 2, 2).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].scope, "test1");

    // Querying wildcard results with no matches
    let results = DocumentBox::search_query(&db, "1test:%", 0, 5)
        .await
        .unwrap();
    assert!(results.is_empty());
}

/// Tests that a search query can be used to search for document boxes total
#[tokio::test]
async fn test_document_box_search_total() {
    let (db, _db_container) = test_tenant_db().await;

    // Initial query result should be empty
    let results = DocumentBox::search_total(&db, "test").await.unwrap();
    assert_eq!(results, 0);

    // Querying specific result
    DocumentBox::create(&db, "test1".to_string()).await.unwrap();
    DocumentBox::create(&db, "test2".to_string()).await.unwrap();
    DocumentBox::create(&db, "test3".to_string()).await.unwrap();

    let results = DocumentBox::search_total(&db, "test3").await.unwrap();
    assert_eq!(results, 1);

    // Querying wildcard results
    let results = DocumentBox::search_total(&db, "test%").await.unwrap();
    assert_eq!(results, 3);

    // Querying wildcard results with no matches
    let results = DocumentBox::search_total(&db, "1test:%").await.unwrap();
    assert_eq!(results, 0);
}
