use std::str::{FromStr, ParseBoolError};

use thiserror::Error;
use tracing::Subscriber;
use tracing_subscriber::{EnvFilter, Layer, fmt, registry::LookupSpan};

/// Logging format to use
#[derive(Debug, Default)]
pub enum LoggingFormat {
    Text,
    #[default]
    Json,
}

#[derive(Debug, Error)]
#[error("unknown logging format: {0}")]
pub struct UnknownLoggingFormat(String);

impl FromStr for LoggingFormat {
    type Err = UnknownLoggingFormat;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "text" => Ok(LoggingFormat::Text),
            "json" => Ok(LoggingFormat::Json),
            value => Err(UnknownLoggingFormat(value.to_string())),
        }
    }
}

impl LoggingFormat {
    pub fn from_env() -> Result<LoggingFormat, UnknownLoggingFormat> {
        let format = match std::env::var("DOCBOX_LOGGING_FORMAT") {
            Ok(value) => value,
            Err(_) => return Ok(LoggingFormat::default()),
        };

        format.parse()
    }
}

#[derive(Debug, Default)]
pub struct LoggingFormatConfig {
    pub format: LoggingFormat,
    pub allow_noisy: bool,
}

#[derive(Debug, Error)]
pub enum LoggingFormatConfigError {
    #[error(transparent)]
    LoggingFormat(#[from] UnknownLoggingFormat),
    #[error("failed to parse DOCBOX_LOGGING_ALLOW_NOISY")]
    InvalidAllowNoisy(ParseBoolError),
}

impl LoggingFormatConfig {
    pub fn from_env() -> Result<Self, LoggingFormatConfigError> {
        let format = LoggingFormat::from_env()?;
        let allow_noisy = std::env::var("DOCBOX_LOGGING_ALLOW_NOISY")
            .ok()
            .map(|value| value.parse::<bool>())
            .transpose()
            .map_err(LoggingFormatConfigError::InvalidAllowNoisy)?
            .unwrap_or_default();

        Ok(Self {
            format,
            allow_noisy,
        })
    }
}

/// Create a formatting layer from the provided format config
pub fn fmt_layer<S>(config: LoggingFormatConfig) -> Box<dyn Layer<S> + Send + Sync + 'static>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    match config.format {
        LoggingFormat::Text => text_fmt_layer().boxed(),
        LoggingFormat::Json => json_fmt_layer().boxed(),
    }
}

/// Layer the outputs content using a text based formatting
pub fn text_fmt_layer<S>() -> fmt::Layer<S> {
    fmt::layer()
        // Display source code file paths
        .with_file(true)
        // Display source code line numbers
        .with_line_number(true)
        // Don't display the event's target (module path)
        .with_target(false)
}

/// Layer that outputs content using a JSON formatting
pub fn json_fmt_layer<S>()
-> fmt::Layer<S, fmt::format::JsonFields, fmt::format::Format<fmt::format::Json>> {
    fmt::layer::<S>()
        .json()
        .with_span_list(true)
        // Display source code file paths
        .with_file(true)
        // Display source code line numbers
        .with_line_number(true)
        // Don't display the event's target (module path)
        .with_target(false)
}

pub fn filter_layer(allow_noisy: bool) -> EnvFilter {
    if allow_noisy {
        return EnvFilter::from_default_env();
    }

    EnvFilter::from_default_env()
        // Increase logging requirements for noisy dependencies
        .add_directive(
            "aws_sdk_secretsmanager=info"
                .parse()
                .expect("directive was invalid"),
        )
        .add_directive("aws_runtime=info".parse().expect("directive was invalid"))
        .add_directive(
            "aws_smithy_runtime=info"
                .parse()
                .expect("directive was invalid"),
        )
        .add_directive("hyper_util=info".parse().expect("directive was invalid"))
        .add_directive("aws_sdk_sqs=info".parse().expect("directive was invalid"))
        .add_directive("h2=info".parse().expect("directive was invalid"))
}
