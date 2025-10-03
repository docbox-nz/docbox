use crate::common::typesense::test_typesense;

mod common;

/// Tests that a typesense search index can be created
#[tokio::test]
async fn test_create_typesense_index() {
    let (_container, index) = test_typesense().await;

    index.create_index().await.unwrap();
}

/// Tests that a typesense search index can be deleted
#[tokio::test]
async fn test_delete_typesense_index() {
    let (_container, index) = test_typesense().await;

    index.create_index().await.unwrap();
    index.delete_index().await.unwrap();
}

/// Tests that index_exists() reports the correct state for an index
#[tokio::test]
async fn test_delete_typesense_index_exists() {
    let (_container, index) = test_typesense().await;

    index.create_index().await.unwrap();

    let exists = index.index_exists().await.unwrap();
    assert!(exists);

    index.delete_index().await.unwrap();

    let exists = index.index_exists().await.unwrap();
    assert!(!exists);
}
