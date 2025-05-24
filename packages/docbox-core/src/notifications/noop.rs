use super::{NotificationQueue, NotificationQueueMessage};

/// Notification queue that will only reply with [None] for
/// cases when a queue is not available
pub struct NoopNotificationQueue;

impl NotificationQueue for NoopNotificationQueue {
    async fn next_message(&mut self) -> Option<NotificationQueueMessage> {
        None
    }
}
