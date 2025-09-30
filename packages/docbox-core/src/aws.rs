use aws_config::{BehaviorVersion, SdkConfig, meta::region::RegionProviderChain};

pub type SqsClient = aws_sdk_sqs::Client;

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

/// Create the AWS production configuration using a specific AWS_PROFILE
pub async fn aws_config_with_profile(profile_name: impl Into<String>) -> SdkConfig {
    let region_provider = RegionProviderChain::default_provider()
        // Fallback to our desired region
        .or_else("ap-southeast-2");

    // Load the configuration from env variables (See https://docs.aws.amazon.com/sdkref/latest/guide/settings-reference.html#EVarSettings)
    aws_config::from_env()
        // Setup the region provider
        .region(region_provider)
        .profile_name(profile_name)
        .behavior_version(BehaviorVersion::v2025_01_17())
        .load()
        .await
}
