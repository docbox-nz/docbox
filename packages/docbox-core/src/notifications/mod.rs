//! # Notifications
//!
//! Notifications queue system handling notifications for the app

use crate::aws::SqsClient;
mod mpsc;
mod noop;
pub mod process;
mod sqs;

pub use mpsc::MpscNotificationQueueSender;

use serde::Deserialize;

// Pretty common utility function
pub use sqs::parse_bucket_message;

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum NotificationConfig {
    Sqs { queue_url: String },
    Noop,
    Mpsc,
}

impl NotificationConfig {
    pub fn from_env() -> Self {
        match (
            std::env::var("DOCBOX_MPSC_QUEUE"),
            std::env::var("DOCBOX_SQS_URL"),
        ) {
            (Ok(_), _) => NotificationConfig::Mpsc,
            (_, Ok(queue_url)) => NotificationConfig::Sqs { queue_url },
            _ => NotificationConfig::Noop,
        }
    }
}

pub enum AppNotificationQueue {
    Sqs(sqs::SqsNotificationQueue),
    Noop(noop::NoopNotificationQueue),
    Mpsc(mpsc::MpscNotificationQueue),
}

impl AppNotificationQueue {
    pub fn from_config(sqs_client: SqsClient, config: NotificationConfig) -> Self {
        match config {
            NotificationConfig::Sqs { queue_url } => {
                tracing::debug!(%queue_url, "using SQS notification queue");
                AppNotificationQueue::Sqs(sqs::SqsNotificationQueue::create(sqs_client, queue_url))
            }
            NotificationConfig::Noop => {
                tracing::warn!("queue not specified, falling back to no-op queue");
                AppNotificationQueue::Noop(noop::NoopNotificationQueue)
            }
            NotificationConfig::Mpsc => {
                tracing::debug!("DOCBOX_MPSC_QUEUE is set using local webhook notification queue");
                AppNotificationQueue::Mpsc(mpsc::MpscNotificationQueue::create())
            }
        }
    }

    pub async fn next_message(&mut self) -> Option<NotificationQueueMessage> {
        match self {
            AppNotificationQueue::Sqs(queue) => queue.next_message().await,
            AppNotificationQueue::Noop(queue) => queue.next_message().await,
            AppNotificationQueue::Mpsc(queue) => queue.next_message().await,
        }
    }
}

/// Type of message from the notification queue
pub enum NotificationQueueMessage {
    FileCreated {
        bucket_name: String,
        object_key: String,
    },
}

pub(crate) trait NotificationQueue: Send + Sync + 'static {
    /// Request the next message from the notification queue
    async fn next_message(&mut self) -> Option<NotificationQueueMessage>;
}
