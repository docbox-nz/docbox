use std::error::Error;

use aws_config::SdkConfig;
use tracing_cloudwatch::CloudWatchWorkerGuard;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::logging::{
    cloudwatch::cloudwatch_layer,
    config::LoggingConfig,
    fmt::{filter_layer, fmt_layer},
    sentry::sentry_layer,
};

pub mod cloudwatch;
pub mod config;
pub mod fmt;
pub mod sentry;

/// Guards that must be held for active loggers
#[derive(Default)]
pub struct LoggingGuards {
    sentry: Option<::sentry::ClientInitGuard>,
    cloudwatch: Option<CloudWatchWorkerGuard>,
}

pub fn init_logging(
    aws_config: &SdkConfig,
    config: LoggingConfig,
) -> Result<LoggingGuards, Box<dyn Error>> {
    let mut guards = LoggingGuards::default();

    let filter_layer = filter_layer(config.format.allow_noisy);

    let mut sentry = None;
    let mut cloudwatch = None;

    if let Some((sentry_layer, client_init_guard)) = sentry_layer(config.sentry) {
        guards.sentry = Some(client_init_guard);
        sentry = Some(sentry_layer);
    }

    if let Some((cloudwatch_layer, worker_guard)) = cloudwatch_layer(aws_config, config.cloudwatch)
    {
        guards.cloudwatch = Some(worker_guard);
        cloudwatch = Some(cloudwatch_layer);
    }

    tracing_subscriber::registry()
        .with(fmt_layer(config.format))
        .with(sentry)
        .with(cloudwatch)
        .with(filter_layer)
        .init();

    Ok(guards)
}
