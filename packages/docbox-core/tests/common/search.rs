use std::sync::Arc;

use docbox_core::aws::aws_config;
use docbox_database::models::tenant::Tenant;
use docbox_search::{
    SearchIndexFactory, SearchIndexFactoryConfig, TenantSearchIndex, TypesenseApiKey,
    TypesenseSearchConfig,
};
use docbox_secrets::{AppSecretManager, memory::MemorySecretManager};
use testcontainers::{
    GenericImage, ImageExt,
    core::{IntoContainerPort, WaitFor, wait::HttpWaitStrategy},
};
use testcontainers_modules::testcontainers::ContainerAsync;
use uuid::Uuid;

/// Testing utility to create and setup a search index for a tenant to use in tests that
/// require search access
///
/// Requires that the test runner have docker available to launch the typesense
/// container that will be used
///
/// Marked with #[allow(dead_code)] as it is used by tests but
/// rustc doesn't believe us
#[allow(dead_code)]
pub async fn create_test_tenant_typesense() -> (ContainerAsync<GenericImage>, TenantSearchIndex) {
    use testcontainers_modules::testcontainers::runners::AsyncRunner;

    let api_key = "typesensedev";

    let container = GenericImage::new("typesense/typesense", "28.0")
        .with_exposed_port(8108.tcp())
        .with_wait_for(WaitFor::seconds(5))
        .with_wait_for(WaitFor::http(
            HttpWaitStrategy::new("/health").with_expected_status_code(200u16),
        ))
        .with_env_var("TYPESENSE_API_KEY", api_key)
        .with_env_var("TYPESENSE_DATA_DIR", "/data")
        .with_mount(testcontainers::core::Mount::tmpfs_mount("/data"))
        .start()
        .await
        .unwrap();

    let host = container.get_host().await.unwrap();
    let host_port = container.get_host_port_ipv4(8108).await.unwrap();
    let url = format!("http://{host}:{host_port}");

    let config = SearchIndexFactoryConfig::Typesense(TypesenseSearchConfig {
        url,
        api_key: Some(TypesenseApiKey::new(api_key.to_string())),
        api_key_secret_name: None,
    });

    let aws_config = aws_config().await;
    let secrets = AppSecretManager::Memory(MemorySecretManager::default());

    let index = SearchIndexFactory::from_config(&aws_config, Arc::new(secrets), config).unwrap();
    let index = index.create_search_index(&Tenant {
        id: Uuid::new_v4(),
        name: "test".to_string(),
        db_name: "test".to_string(),
        db_secret_name: "test".to_string(),
        s3_name: "test".to_string(),
        os_index_name: "test".to_string(),
        env: "Development".to_string(),
        event_queue_url: None,
    });

    // Setup the index
    index.create_index().await.unwrap();

    (container, index)
}
