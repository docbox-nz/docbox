use tokio::sync::mpsc;

use super::{EventPublisher, TenantEventMessage};

/// In memory multi-producer single-consumer event channel, used for
/// event handling in tests
#[derive(Clone)]
pub struct MpscEventPublisher {
    tx: mpsc::UnboundedSender<TenantEventMessage>,
}

impl MpscEventPublisher {
    pub fn new() -> (Self, mpsc::UnboundedReceiver<TenantEventMessage>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { tx }, rx)
    }
}

impl EventPublisher for MpscEventPublisher {
    fn publish_event(&self, event: super::TenantEventMessage) {
        tracing::debug!(?event, "mpsc tenant event");
        _ = self.tx.send(event);
    }
}
