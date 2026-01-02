use super::{EventPublisher, TenantEventMessage};
use aws_sdk_sqs::Client as SqsClient;
use docbox_database::models::tenant::TenantId;
use serde::Serialize;
use tracing::Instrument;

#[derive(Clone)]
pub struct SqsEventPublisherFactory {
    client: SqsClient,
}

impl SqsEventPublisherFactory {
    pub fn new(client: SqsClient) -> Self {
        Self { client }
    }

    pub fn create_event_publisher(&self, target: TenantSqsEventQueue) -> SqsEventPublisher {
        SqsEventPublisher {
            client: self.client.clone(),
            target,
        }
    }
}

/// Tenant event publisher that publishes events through SQS
#[derive(Clone)]
pub struct SqsEventPublisher {
    client: SqsClient,
    target: TenantSqsEventQueue,
}

/// Target SQS details queue
#[derive(Clone)]
pub struct TenantSqsEventQueue {
    pub tenant_id: TenantId,
    pub event_queue_url: String,
}

/// Container around an event message containing the ID of the
/// tenant that the message occurred within for multi-tenanted
/// event handling
///
/// i.e { "event": "DOCUMENT_BOX_CREATED", "data": { ...document box data }, "tenant_id": "xxxxx-xxxxx-xxxxx-xxxxx" }
#[derive(Debug, Serialize)]
struct TenantEventMessageContainer {
    tenant_id: TenantId,
    #[serde(flatten)]
    message: TenantEventMessage,
}

impl EventPublisher for SqsEventPublisher {
    fn publish_event(&self, event: TenantEventMessage) {
        let client = self.client.clone();
        let tenant_id = self.target.tenant_id;
        let event_queue_url = self.target.event_queue_url.clone();

        // Wrap the event message providing the tenant_id
        let event = TenantEventMessageContainer {
            message: event,
            tenant_id,
        };

        let span = tracing::Span::current();

        tokio::spawn(
            async move {
                // Serialize the event message
                let msg = match serde_json::to_string(&event) {
                    Ok(value) => value,
                    Err(error) => {
                        tracing::error!(?error, ?event, "failed to serialize tenant event");
                        return;
                    }
                };

                tracing::debug!(?event, "emitting tenant event");

                // Push the event to the SQS queue
                if let Err(error) = client
                    .send_message()
                    .queue_url(event_queue_url)
                    .message_body(msg)
                    .send()
                    .await
                {
                    tracing::error!(?error, ?event, "failed to emit tenant event");
                }
            }
            .instrument(span),
        );
    }
}
