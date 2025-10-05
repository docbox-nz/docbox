use std::{collections::HashMap, sync::Arc};

use docbox_search::{SearchIndexFactory, TypesenseIndexFactory, TypesenseSearchConfig};
use docbox_secrets::{Secret, SecretManager, memory::MemorySecretManager};

use crate::common::{
    tenant::test_tenant,
    typesense::{TEST_API_KEY, test_typesense_container},
};

mod common;

/// Tests that a typesense API key can be loaded from a secrets manager
/// secret and used with a request
#[tokio::test]
async fn test_typesense_api_secret() {
    let container = test_typesense_container().await;

    let host = container.get_host().await.unwrap();
    let host_port = container.get_host_port_ipv4(8108).await.unwrap();
    let url = format!("http://{host}:{host_port}");

    let secret_name = "typesense/test-secret";

    let config = TypesenseSearchConfig {
        url,
        api_key: None,
        api_key_secret_name: Some(secret_name.to_string()),
    };

    // Make a secret manager with the required secret
    let memory = MemorySecretManager::new(
        [(
            secret_name.to_string(),
            Secret::String(TEST_API_KEY.to_string()),
        )]
        .into_iter()
        .collect::<HashMap<_, _>>(),
        None,
    );

    let secrets = Arc::new(SecretManager::Memory(memory));

    let search = TypesenseIndexFactory::from_config(secrets, config)
        .map(SearchIndexFactory::Typesense)
        .unwrap();
    let index = search.create_search_index(&test_tenant());

    // Test that the client works (API key is lazily initialized)
    index.create_index().await.unwrap();
    index.delete_index().await.unwrap();
}
