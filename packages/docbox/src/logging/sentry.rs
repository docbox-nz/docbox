use sentry_tracing::SentryLayer;
use tracing::{Level, Subscriber};
use tracing_subscriber::registry::LookupSpan;

use crate::logging::config::LoggingConfigError;

/// Configuration for sentry logging
#[derive(Default)]
pub struct SentryLoggingConfig {
    pub dsn: Option<String>,
}

impl SentryLoggingConfig {
    pub fn from_env() -> Result<Self, LoggingConfigError> {
        let dsn = std::env::var("SENTRY_DSN")
            .ok()
            .or(std::env::var("DOCBOX_SENTRY_DSN").ok());

        Ok(Self { dsn })
    }
}

pub fn sentry_layer<S>(
    config: SentryLoggingConfig,
) -> Option<(SentryLayer<S>, sentry::ClientInitGuard)>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    let dsn = config.dsn?;
    let options = sentry::ClientOptions {
        release: sentry::release_name!(),
        ..Default::default()
    };
    let client_init_guard = sentry::init((dsn, options));

    let sentry_layer = sentry_tracing::layer()
        .event_filter(|event| {
            match event.level() {
                &Level::ERROR => {
                    // Ignore errors emitted from the docbox_web_scraper when emitting
                    // errors to sentry (These are errors caused by the upstream site)
                    if let Some(module_path) = event.module_path()
                        && module_path.starts_with("docbox_web_scraper")
                    {
                        return sentry_tracing::EventFilter::Ignore;
                    }

                    sentry_tracing::EventFilter::Event
                }
                &Level::WARN | &Level::INFO => sentry_tracing::EventFilter::Breadcrumb,
                &Level::DEBUG | &Level::TRACE => sentry_tracing::EventFilter::Ignore,
            }
        })
        .enable_span_attributes();

    Some((sentry_layer, client_init_guard))
}
