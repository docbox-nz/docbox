use crate::common::{
    tenant::test_tenant,
    typesense::{test_search_factory, test_typesense_container},
};

mod common;

/// Tests that index_exists() reports the correct state for an index
#[tokio::test]
async fn test_typesense_index_exists() {
    let container = test_typesense_container().await;
    let search = test_search_factory(&container).await;
    let index = search.create_search_index(&test_tenant());

    index.create_index().await.unwrap();

    let exists = index.index_exists().await.unwrap();
    assert!(exists);

    index.delete_index().await.unwrap();

    let exists = index.index_exists().await.unwrap();
    assert!(!exists);
}
