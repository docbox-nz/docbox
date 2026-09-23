use aws_config::{BehaviorVersion, Region, SdkConfig};
use docbox_storage::s3::S3StorageLayerFactory;
use docbox_storage::{StorageLayerFactory, s3::S3StorageLayerFactoryConfig};
use testcontainers::ImageExt;
use testcontainers::core::WaitFor;
use testcontainers::core::wait::HttpWaitStrategy;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::{rustfs::RustFS, testcontainers::ContainerAsync};

const TEST_ACCESS_KEY: &str = "rustadmin";
const TEST_ACCESS_SECRET: &str = "rustadmin";

/// Create a new [RustFS](https://rustfs.com) container for testing
pub async fn test_rustfs_container() -> ContainerAsync<RustFS> {
    RustFS::default()
        .with_env_var("RUSTFS_ACCESS_KEY", TEST_ACCESS_KEY)
        .with_env_var("RUSTFS_SECRET_KEY", TEST_ACCESS_SECRET)
        .with_ready_conditions(vec![WaitFor::http(
            HttpWaitStrategy::new("/health/ready")
                .with_response_matcher(|response| response.status().as_u16() == 200),
        )])
        .start()
        .await
        .unwrap()
}

/// Create an AWS sdk config for use in tests
fn test_sdk_config() -> SdkConfig {
    SdkConfig::builder()
        .behavior_version(BehaviorVersion::v2026_01_12())
        .region(Region::from_static("us-east-1"))
        .build()
}

/// Create a new storage factory based on the provided rustfs container
pub async fn test_storage_factory(container: &ContainerAsync<RustFS>) -> StorageLayerFactory {
    let host = container.get_host().await.unwrap();
    let host_port = container.get_host_port_ipv4(9000).await.unwrap();
    let url = format!("http://{host}:{host_port}");

    // Setup storage factory
    let aws_config = test_sdk_config();

    let endpoint = docbox_storage::s3::S3Endpoint::Custom {
        endpoint: url,
        external_endpoint: None,
        access_key_id: TEST_ACCESS_KEY.to_string(),
        access_key_secret: TEST_ACCESS_SECRET.to_string(),
    };

    let config = S3StorageLayerFactoryConfig { endpoint };

    StorageLayerFactory::S3(S3StorageLayerFactory::from_config(&aws_config, config))
}
