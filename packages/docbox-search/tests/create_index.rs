use crate::common::{
    tenant::test_tenant,
    typesense::{test_search_factory, test_typesense_container},
};

mod common;

/// Tests that a typesense search index can be created
#[tokio::test]
async fn test_create_typesense_index() {
    let container = test_typesense_container().await;
    let search = test_search_factory(&container).await;
    let index = search.create_search_index(&test_tenant());

    index.create_index().await.unwrap();
}
