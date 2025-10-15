use super::EventPublisher;

/// No-op event publisher that doesn't send the event anywhere. For
/// tenants that don't support event publishing
#[derive(Default, Clone)]
pub struct NoopEventPublisher;

impl EventPublisher for NoopEventPublisher {
    fn publish_event(&self, event: super::TenantEventMessage) {
        tracing::debug!(?event, "no-op tenant event");
    }
}
