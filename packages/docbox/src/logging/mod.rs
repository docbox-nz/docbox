use std::error::Error;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::logging::{
    config::LoggingConfig,
    fmt::{filter_layer, fmt_layer},
    sentry::sentry_layer,
};

pub mod config;
pub mod fmt;
pub mod sentry;

/// Guards that must be held for active loggers
#[derive(Default)]
pub struct LoggingGuards {
    sentry: Option<::sentry::ClientInitGuard>,
}

pub fn init_logging(config: LoggingConfig) -> Result<LoggingGuards, Box<dyn Error>> {
    let mut guards = LoggingGuards::default();

    let filter_layer = filter_layer(config.format.allow_noisy);

    let mut sentry = None;

    if let Some((sentry_layer, client_init_guard)) = sentry_layer(config.sentry) {
        guards.sentry = Some(client_init_guard);
        sentry = Some(sentry_layer);
    }

    tracing_subscriber::registry()
        .with(fmt_layer(config.format))
        .with(sentry)
        .with(filter_layer)
        .init();

    Ok(guards)
}
