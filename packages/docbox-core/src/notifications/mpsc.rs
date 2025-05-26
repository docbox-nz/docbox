use super::{NotificationQueue, NotificationQueueMessage};
use tokio::sync::mpsc;

/// In-process notification queue, used for a webhook based system when using
/// a webhook endpoint on the server as a notification source
pub struct MpscNotificationQueue {
    rx: mpsc::Receiver<NotificationQueueMessage>,
}

#[derive(Clone)]
pub struct MpscNotificationQueueSender {
    tx: mpsc::Sender<NotificationQueueMessage>,
}

impl MpscNotificationQueueSender {
    pub async fn send(&self, msg: NotificationQueueMessage) {
        _ = self.tx.send(msg).await;
    }
}

impl MpscNotificationQueue {
    pub fn create() -> (MpscNotificationQueue, MpscNotificationQueueSender) {
        let (tx, rx) = mpsc::channel(10);
        (
            MpscNotificationQueue { rx },
            MpscNotificationQueueSender { tx },
        )
    }
}

impl NotificationQueue for MpscNotificationQueue {
    async fn next_message(&mut self) -> Option<NotificationQueueMessage> {
        self.rx.recv().await
    }
}
