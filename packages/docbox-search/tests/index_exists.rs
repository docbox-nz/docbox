use crate::common::typesense::test_typesense;

mod common;

/// Tests that index_exists() reports the correct state for an index
#[tokio::test]
async fn test_typesense_index_exists() {
    let (_container, index) = test_typesense().await;

    index.create_index().await.unwrap();

    let exists = index.index_exists().await.unwrap();
    assert!(exists);

    index.delete_index().await.unwrap();

    let exists = index.index_exists().await.unwrap();
    assert!(!exists);
}
