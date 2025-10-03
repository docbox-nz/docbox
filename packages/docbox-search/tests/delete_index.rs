use crate::common::typesense::test_typesense;

mod common;

/// Tests that a typesense search index can be deleted
#[tokio::test]
async fn test_delete_typesense_index() {
    let (_container, index) = test_typesense().await;

    index.create_index().await.unwrap();
    index.delete_index().await.unwrap();
}
