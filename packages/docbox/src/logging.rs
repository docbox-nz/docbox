use tracing::Level;
use tracing_subscriber::{EnvFilter, fmt::Layer, layer::SubscriberExt, util::SubscriberInitExt};

pub fn init_logging_with_sentry(dsn: String) -> anyhow::Result<sentry::ClientInitGuard> {
    let options = sentry::ClientOptions {
        release: sentry::release_name!(),
        ..Default::default()
    };
    let sentry = sentry::init((dsn, options));

    let sentry_layer = sentry_tracing::layer()
        .event_filter(|event| {
            match event.level() {
                &Level::ERROR => {
                    // Ignore errors emitted from the docbox_web_scraper when emitting
                    // errors to sentry (These are errors caused by the upstream site)
                    if let Some(module_path) = event.module_path() {
                        if module_path.starts_with("docbox_web_scraper") {
                            return sentry_tracing::EventFilter::Ignore;
                        }
                    }

                    sentry_tracing::EventFilter::Event
                }
                &Level::WARN | &Level::INFO => sentry_tracing::EventFilter::Breadcrumb,
                &Level::DEBUG | &Level::TRACE => sentry_tracing::EventFilter::Ignore,
            }
        })
        .enable_span_attributes();

    tracing_subscriber::registry()
        .with(filter()?)
        .with(fmt_layer())
        .with(sentry_layer)
        .init();

    Ok(sentry)
}

pub fn init_logging() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(filter()?)
        .with(fmt_layer())
        .init();
    Ok(())
}

pub fn fmt_layer<S>() -> Layer<S> {
    tracing_subscriber::fmt::layer()
        // Display source code file paths
        .with_file(true)
        // Display source code line numbers
        .with_line_number(true)
        // Don't display the event's target (module path)
        .with_target(false)
}

pub fn filter() -> anyhow::Result<EnvFilter> {
    // Use the logging options from env variables
    let filter = EnvFilter::from_default_env()
        // Increase logging requirements for noisy dependencies
        .add_directive("aws_sdk_secretsmanager=info".parse()?)
        .add_directive("aws_runtime=info".parse()?)
        .add_directive("aws_smithy_runtime=info".parse()?)
        .add_directive("hyper_util=info".parse()?)
        .add_directive("aws_sdk_sqs=info".parse()?)
        .add_directive("h2=info".parse()?);
    Ok(filter)
}
