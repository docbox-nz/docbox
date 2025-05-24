use aws_config::{meta::region::RegionProviderChain, SdkConfig};

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
        .load()
        .await
}

/// FOR LOCAL MOCK ONLY
///
/// Enforces the "path" style for S3 bucket access. Required for the local s3mock
/// https://github.com/adobe/S3Mock#path-style-vs-domain-style-access since it does
/// not support the domain style buckets.
///
/// Without this testing locally with s3mock will fail when attempting to create buckets
pub fn create_s3_client_dev() -> S3Client {
    let config = aws_sdk_s3::config::Builder::new()
        .force_path_style(true)
        .endpoint_url(std::env::var("AWS_ENDPOINT_URL_S3").unwrap())
        .region(aws_config::Region::new(
            std::env::var("AWS_REGION").unwrap(),
        ))
        .behavior_version(aws_config::BehaviorVersion::latest())
        .build();

    S3Client::from_conf(config)
}
