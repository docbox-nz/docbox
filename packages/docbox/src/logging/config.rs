use thiserror::Error;

use crate::logging::{
    fmt::{LoggingFormatConfig, LoggingFormatConfigError},
    sentry::SentryLoggingConfig,
};

/// Configuration for logging
#[derive(Default)]
pub struct LoggingConfig {
    pub format: LoggingFormatConfig,
    pub sentry: SentryLoggingConfig,
}

#[derive(Debug, Error)]
pub enum LoggingConfigError {
    #[error(transparent)]
    LoggingFormatConfig(#[from] LoggingFormatConfigError),
}

impl LoggingConfig {
    pub fn from_env() -> Result<Self, LoggingConfigError> {
        let format = LoggingFormatConfig::from_env()?;
        let sentry = SentryLoggingConfig::from_env()?;
        Ok(Self { format, sentry })
    }
}
