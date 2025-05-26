use anyhow::Context;
use aws_config::{meta::region::RegionProviderChain, BehaviorVersion, SdkConfig};
use aws_sdk_s3::config::Credentials;

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

pub fn s3_client_from_env(config: &SdkConfig) -> anyhow::Result<S3Client> {
    match std::env::var("DOCBOX_S3_ENDPOINT") {
        // Using a custom S3 endpoint
        Ok(endpoint_url) => {
            tracing::debug!(
                ?endpoint_url,
                "DOCBOX_S3_ENDPOINT is set, overriding S3 endpoint"
            );

            let access_key_id = std::env::var("DOCBOX_S3_ACCESS_KEY_ID").context(
                "cannot use DOCBOX_S3_ENDPOINT without specifying DOCBOX_S3_ACCESS_KEY_ID",
            )?;
            let access_key_secret = std::env::var("DOCBOX_S3_ACCESS_KEY_SECRET").context(
                "cannot use DOCBOX_S3_ENDPOINT without specifying DOCBOX_S3_ACCESS_KEY_SECRET",
            )?;

            let credentials = Credentials::new(
                access_key_id,
                access_key_secret,
                None,
                None,
                "docbox_key_provider",
            );

            // Enforces the "path" style for S3 bucket access
            let config = aws_sdk_s3::config::Builder::from(config)
                .force_path_style(true)
                .endpoint_url(endpoint_url)
                .credentials_provider(credentials)
                .build();

            Ok(S3Client::from_conf(config))
        }
        Err(_) => Ok(S3Client::new(config)),
    }
}
