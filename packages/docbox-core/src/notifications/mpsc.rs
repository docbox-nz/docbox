use super::{NotificationQueue, NotificationQueueMessage};
use tokio::sync::mpsc;

/// In-process notification queue, used for a webhook based system when using
/// a webhook endpoint on the server as a notification source
pub struct MpscNotificationQueue {
    rx: mpsc::Receiver<NotificationQueueMessage>,

    /// Sender held by the queue until its consumed
    sender: Option<MpscNotificationQueueSender>,
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
    pub fn create() -> MpscNotificationQueue {
        let (tx, rx) = mpsc::channel(10);
        MpscNotificationQueue {
            rx,
            sender: Some(MpscNotificationQueueSender { tx }),
        }
    }

    pub fn take_sender(&mut self) -> Option<MpscNotificationQueueSender> {
        self.sender.take()
    }
}

impl NotificationQueue for MpscNotificationQueue {
    async fn next_message(&mut self) -> Option<NotificationQueueMessage> {
        self.rx.recv().await
    }
}
