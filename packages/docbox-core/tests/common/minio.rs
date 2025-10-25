use aws_config::{BehaviorVersion, Region, SdkConfig};
use docbox_database::models::tenant::Tenant;
use docbox_storage::TenantStorageLayer;
use docbox_storage::s3::S3StorageLayerFactory;
use docbox_storage::{StorageLayerFactory, s3::S3StorageLayerFactoryConfig};
use testcontainers::ImageExt;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::{minio::MinIO, testcontainers::ContainerAsync};

const TEST_MINIO_USER: &str = "minioadmin";
const TEST_MINIO_PASSWORD: &str = "minioadmin";

/// Create a new [Minio](https://www.min.io/) container for testing
pub async fn test_minio_container() -> ContainerAsync<MinIO> {
    MinIO::default()
        .with_env_var("MINIO_ROOT_USER", TEST_MINIO_USER)
        .with_env_var("MINIO_ROOT_PASSWORD", TEST_MINIO_PASSWORD)
        .start()
        .await
        .unwrap()
}

/// Create an AWS sdk config for use in tests
fn test_sdk_config() -> SdkConfig {
    SdkConfig::builder()
        .behavior_version(BehaviorVersion::v2025_08_07())
        .region(Region::from_static("us-east-1"))
        .build()
}

/// Create a new storage factory based on the provided minio container
pub async fn test_storage_factory(container: &ContainerAsync<MinIO>) -> StorageLayerFactory {
    let host = container.get_host().await.unwrap();
    let host_port = container.get_host_port_ipv4(9000).await.unwrap();
    let url = format!("http://{host}:{host_port}");

    // Setup storage factory
    let aws_config = test_sdk_config();

    let endpoint = docbox_storage::s3::S3Endpoint::Custom {
        endpoint: url,
        access_key_id: TEST_MINIO_USER.to_string(),
        access_key_secret: TEST_MINIO_PASSWORD.to_string(),
    };

    let config = S3StorageLayerFactoryConfig { endpoint };

    StorageLayerFactory::S3(S3StorageLayerFactory::from_config(&aws_config, config))
}

#[allow(dead_code)]
pub async fn test_tenant_storage(tenant: &Tenant) -> (TenantStorageLayer, ContainerAsync<MinIO>) {
    let storage_container = test_minio_container().await;
    let storage = test_storage_factory(&storage_container).await;
    let storage = storage.create_storage_layer(tenant);
    storage.create_bucket().await.unwrap();

    (storage, storage_container)
}
