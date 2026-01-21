use aws_config::{BehaviorVersion, Region, SdkConfig};
use docbox_secrets::{
    SecretManager,
    aws::{AwsSecretManager, AwsSecretManagerConfig, AwsSecretsEndpoint},
};
use testcontainers::{
    ContainerAsync, GenericImage, ImageExt,
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
};

const TEST_ENCRYPTION_KEY: &str = "test";
const TEST_ACCESS_KEY_ID: &str = "test";
const TEST_ACCESS_KEY_SECRET: &str = "test";

/// Create an AWS sdk config for use in tests
#[allow(dead_code)]
pub fn test_sdk_config() -> SdkConfig {
    SdkConfig::builder()
        .behavior_version(BehaviorVersion::v2026_01_12())
        .region(Region::from_static("us-east-1"))
        .build()
}

/// Create a new [Loker](https://github.com/jacobtread/loker) container for testing
#[allow(dead_code)]
pub async fn test_loker_container() -> ContainerAsync<GenericImage> {
    GenericImage::new("jacobtread/loker", "0.2.2")
        .with_exposed_port(8080.tcp())
        .with_wait_for(WaitFor::seconds(5))
        .with_env_var("SM_ENCRYPTION_KEY", TEST_ENCRYPTION_KEY)
        .with_env_var("SM_ACCESS_KEY_ID", TEST_ACCESS_KEY_ID)
        .with_env_var("SM_ACCESS_KEY_SECRET", TEST_ACCESS_KEY_SECRET)
        .start()
        .await
        .unwrap()
}

/// Create a new AWS secrets manager test client from the provided Loker container
#[allow(dead_code)]
pub async fn test_aws_secrets_manager_client(
    container: &ContainerAsync<GenericImage>,
) -> SecretManager {
    let host = container.get_host().await.unwrap();
    let host_port = container.get_host_port_ipv4(8080).await.unwrap();
    let url = format!("http://{host}:{host_port}");
    let aws_config = test_sdk_config();
    SecretManager::Aws(AwsSecretManager::from_config(
        &aws_config,
        AwsSecretManagerConfig {
            endpoint: AwsSecretsEndpoint::Custom {
                endpoint: url,
                access_key_id: TEST_ACCESS_KEY_ID.to_string(),
                access_key_secret: TEST_ACCESS_KEY_SECRET.to_string(),
            },
        },
    ))
}
