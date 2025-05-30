use aws_config::{meta::region::RegionProviderChain, BehaviorVersion, SdkConfig};

pub type SqsClient = aws_sdk_sqs::Client;
pub type S3Client = aws_sdk_s3::Client;
pub type SecretsManagerClient = aws_sdk_secretsmanager::Client;

/// Create the AWS production configuration
pub async fn aws_config() -> SdkConfig {
    let region_provider = RegionProviderChain::default_provider()
        // Fallback to our desired region
        .or_else("ap-southeast-2");

    // Load the configuration from env variables (See https://docs.aws.amazon.com/sdkref/latest/guide/settings-reference.html#EVarSettings)
    aws_config::from_env()
        // Setup the region provider
        .region(region_provider)
        .behavior_version(BehaviorVersion::v2025_01_17())
        .load()
        .await
}
