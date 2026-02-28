use std::{
    num::{NonZeroU64, NonZeroUsize, ParseIntError},
    time::Duration,
};

use aws_config::SdkConfig;
use thiserror::Error;
use tracing::Subscriber;
use tracing_cloudwatch::CloudWatchWorkerGuard;
use tracing_subscriber::{Layer, registry::LookupSpan};

/// Configuration for cloudwatch logging
#[derive(Default)]
pub struct CloudwatchLoggingConfig {
    pub log_group_name: Option<String>,
    pub log_stream_name: Option<String>,
    pub batch_size: Option<NonZeroUsize>,
    pub interval: Option<Duration>,
}

#[derive(Debug, Error)]
pub enum CloudwatchLoggingConfigError {
    #[error("DOCBOX_CLOUDWATCH_BATCH_SIZE must be a positive non-zero number")]
    InvalidBatchSizeNumber(ParseIntError),
    #[error("DOCBOX_CLOUDWATCH_LOG_INTERVAL_SECONDS must be a positive non-zero number")]
    InvalidIntervalNumber(ParseIntError),
}
impl CloudwatchLoggingConfig {
    pub fn from_env() -> Result<Self, CloudwatchLoggingConfigError> {
        let log_group_name = std::env::var("DOCBOX_CLOUDWATCH_LOG_GROUP_NAME").ok();
        let log_stream_name = std::env::var("DOCBOX_CLOUDWATCH_LOG_STREAM_NAME").ok();

        let batch_size = std::env::var("DOCBOX_CLOUDWATCH_LOG_BATCH_SIZE")
            .ok()
            .map(|value| {
                value
                    .parse::<NonZeroUsize>()
                    .map_err(CloudwatchLoggingConfigError::InvalidBatchSizeNumber)
            })
            .transpose()?;

        let interval = std::env::var("DOCBOX_CLOUDWATCH_LOG_INTERVAL_SECONDS")
            .ok()
            .map(|value| {
                let seconds = value
                    .parse::<NonZeroU64>()
                    .map_err(CloudwatchLoggingConfigError::InvalidIntervalNumber)?;
                Ok(Duration::from_secs(seconds.into()))
            })
            .transpose()?;

        Ok(Self {
            log_group_name,
            log_stream_name,
            batch_size,
            interval,
        })
    }
}

/// Layer that pushes logs into cloudwatch
pub fn cloudwatch_layer<S>(
    aws_config: &SdkConfig,
    config: CloudwatchLoggingConfig,
) -> Option<(impl Layer<S>, CloudWatchWorkerGuard)>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    let log_group_name = config.log_group_name?;
    let log_stream_name = config.log_stream_name?;

    let batch_size = config
        .batch_size
        .unwrap_or(NonZeroUsize::new(5).expect("number is non zero"));

    let interval = config.interval.unwrap_or(Duration::from_secs(5));

    let cw_client = aws_sdk_cloudwatchlogs::Client::new(aws_config);

    let fmt_layer = tracing_subscriber::fmt::layer::<S>()
        .json()
        .without_time()
        .with_span_list(true)
        // Display source code file paths
        .with_file(true)
        // Display source code line numbers
        .with_line_number(true)
        // Don't display the event's target (module path)
        .with_target(false);

    Some(
        tracing_cloudwatch::layer()
            .with_fmt_layer(fmt_layer)
            .with_client(
                cw_client,
                tracing_cloudwatch::ExportConfig::default()
                    .with_batch_size(batch_size)
                    .with_interval(interval)
                    .with_log_group_name(log_group_name)
                    .with_log_stream_name(log_stream_name),
            ),
    )
}
