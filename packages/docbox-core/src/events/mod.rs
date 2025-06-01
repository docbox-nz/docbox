//! # Events
//!
//! Event publishing abstraction
//!
//! - [EventPublisherFactory] Factory for building a publisher based on the tenant
//! - [SqsEventPublisherFactory] SQS based event notifications
//! - [NoopEventPublisher] No-op publishing for tenants without event targets

use docbox_database::models::tenant::Tenant;
use docbox_database::models::{
    document_box::{DocumentBox, WithScope},
    file::File,
    folder::Folder,
    link::Link,
};
use serde::Serialize;

pub mod mpsc;
pub mod noop;
pub mod sqs;

use noop::NoopEventPublisher;
use sqs::{SqsEventPublisherFactory, TenantSqsEventQueue};

#[derive(Clone)]
pub struct EventPublisherFactory {
    /// Factory for creating SQS based event publishers
    sqs: SqsEventPublisherFactory,
}

impl EventPublisherFactory {
    pub fn new(sqs: SqsEventPublisherFactory) -> Self {
        Self { sqs }
    }

    pub fn create_event_publisher(&self, tenant: &Tenant) -> TenantEventPublisher {
        match tenant.event_queue_url.as_ref() {
            Some(value) => {
                let target = TenantSqsEventQueue {
                    tenant_id: tenant.id,
                    event_queue_url: value.clone(),
                };

                TenantEventPublisher::Sqs(self.sqs.create_event_publisher(target))
            }
            None => TenantEventPublisher::Noop(NoopEventPublisher),
        }
    }
}

/// Dynamic event publisher for a tenant
#[derive(Clone)]
pub enum TenantEventPublisher {
    Sqs(sqs::SqsEventPublisher),
    Noop(noop::NoopEventPublisher),
    Mpsc(mpsc::MpscEventPublisher),
}

impl TenantEventPublisher {
    pub fn publish_event(&self, event: TenantEventMessage) {
        match self {
            TenantEventPublisher::Sqs(inner) => inner.publish_event(event),
            TenantEventPublisher::Noop(inner) => inner.publish_event(event),
            TenantEventPublisher::Mpsc(inner) => inner.publish_event(event),
        }
    }
}

/// Event inner message type, containing the actual event data
///
/// i.e { "event": "DOCUMENT_BOX_CREATED", "data": { ...document box data }, "tenant_id": "xxxxx-xxxxx-xxxxx-xxxxx" }
#[derive(Debug, Serialize)]
#[serde(tag = "event", content = "data", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TenantEventMessage {
    // Creations (DOCUMENT_BOX_CREATED, ...etc)
    DocumentBoxCreated(DocumentBox),
    FileCreated(WithScope<File>),
    FolderCreated(WithScope<Folder>),
    LinkCreated(WithScope<Link>),

    // Deletions
    DocumentBoxDeleted(DocumentBox),
    FileDeleted(WithScope<File>),
    FolderDeleted(WithScope<Folder>),
    LinkDeleted(WithScope<Link>),
}

/// Abstraction providing the ability to publish an event
pub trait EventPublisher: Send + Sync + 'static {
    /// Publish an event with the event publisher
    fn publish_event(&self, event: TenantEventMessage);
}
