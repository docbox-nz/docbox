use docbox_search::{
    SearchIndexFactory, TypesenseApiKey, TypesenseIndexFactory, TypesenseSearchConfig,
};
use docbox_secrets::{SecretManager, memory::MemorySecretManager};
use testcontainers::{
    GenericImage, ImageExt,
    core::{IntoContainerPort, WaitFor, wait::HttpWaitStrategy},
};
use testcontainers_modules::testcontainers::ContainerAsync;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

pub const TEST_API_KEY: &str = "typesensedev";

/// Create a new [Typesense](https://typesense.org/) container for testing
pub async fn test_typesense_container() -> ContainerAsync<GenericImage> {
    GenericImage::new("typesense/typesense", "28.0")
        .with_exposed_port(8108.tcp())
        .with_wait_for(WaitFor::seconds(5))
        .with_wait_for(WaitFor::http(
            HttpWaitStrategy::new("/health").with_expected_status_code(200u16),
        ))
        .with_env_var("TYPESENSE_API_KEY", TEST_API_KEY)
        .with_env_var("TYPESENSE_DATA_DIR", "/data")
        .with_mount(testcontainers::core::Mount::tmpfs_mount("/data"))
        .start()
        .await
        .unwrap()
}

/// Create a new search factory based on the provided Typesense container
#[allow(dead_code)]
pub async fn test_search_factory(container: &ContainerAsync<GenericImage>) -> SearchIndexFactory {
    let host = container.get_host().await.unwrap();
    let host_port = container.get_host_port_ipv4(8108).await.unwrap();
    let url = format!("http://{host}:{host_port}");

    let config = TypesenseSearchConfig {
        url,
        api_key: Some(TypesenseApiKey::new(TEST_API_KEY.to_string())),
        api_key_secret_name: None,
    };

    let secrets = SecretManager::Memory(MemorySecretManager::default());

    TypesenseIndexFactory::from_config(secrets, config)
        .map(SearchIndexFactory::Typesense)
        .unwrap()
}
