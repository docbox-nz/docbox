use docbox_core::{
    aws::aws_config,
    storage::{
        StorageLayerFactory, StorageLayerFactoryConfig, TenantStorageLayer,
        s3::S3StorageLayerFactoryConfig,
    },
};
use docbox_database::models::tenant::Tenant;
use testcontainers::ImageExt;
use testcontainers_modules::{minio::MinIO, testcontainers::ContainerAsync};
use uuid::Uuid;

/// Testing utility to create and setup a storage bucket for a tenant to use in tests that
/// require storage access
///
/// Requires that the test runner have docker available to launch the minio
/// container that will be used
///
/// Marked with #[allow(dead_code)] as it is used by tests but
/// rustc doesn't believe us
#[allow(dead_code)]
pub async fn create_test_tenant_storage() -> (ContainerAsync<MinIO>, TenantStorageLayer) {
    use testcontainers_modules::testcontainers::runners::AsyncRunner;

    let user = "minioadmin";
    let password = "minioadmin";

    let container = MinIO::default()
        .with_env_var("MINIO_ROOT_USER", user)
        .with_env_var("MINIO_ROOT_PASSWORD", password)
        .start()
        .await
        .unwrap();
    let host = container.get_host().await.unwrap();
    let host_port = container.get_host_port_ipv4(9000).await.unwrap();

    let url = format!("http://{host}:{host_port}");

    // Setup storage factory
    let aws_config = aws_config().await;
    let storage_factory_config = StorageLayerFactoryConfig::S3(S3StorageLayerFactoryConfig {
        endpoint: docbox_core::storage::s3::S3Endpoint::Custom {
            endpoint: url,
            access_key_id: user.to_string(),
            access_key_secret: password.to_string(),
        },
    });
    let storage_factory = StorageLayerFactory::from_config(&aws_config, storage_factory_config);

    let storage = storage_factory.create_storage_layer(&Tenant {
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
    storage.create_bucket().await.unwrap();

    (container, storage)
}
