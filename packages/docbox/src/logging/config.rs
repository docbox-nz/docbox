use thiserror::Error;

use crate::logging::{
    cloudwatch::{CloudwatchLoggingConfig, CloudwatchLoggingConfigError},
    fmt::{LoggingFormatConfig, LoggingFormatConfigError},
    sentry::SentryLoggingConfig,
};

/// Configuration for logging
#[derive(Default)]
pub struct LoggingConfig {
    pub format: LoggingFormatConfig,
    pub sentry: SentryLoggingConfig,
    pub cloudwatch: CloudwatchLoggingConfig,
}

#[derive(Debug, Error)]
pub enum LoggingConfigError {
    #[error(transparent)]
    LoggingFormatConfig(#[from] LoggingFormatConfigError),
    #[error(transparent)]
    CloudwatchLoggingConfig(#[from] CloudwatchLoggingConfigError),
}

impl LoggingConfig {
    pub fn from_env() -> Result<Self, LoggingConfigError> {
        let format = LoggingFormatConfig::from_env()?;
        let sentry = SentryLoggingConfig::from_env()?;
        let cloudwatch = CloudwatchLoggingConfig::from_env()?;

        Ok(Self {
            format,
            sentry,
            cloudwatch,
        })
    }
}
