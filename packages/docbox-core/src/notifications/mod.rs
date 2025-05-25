//! # Notifications
//!
//! Notifications queue system handling notifications for the app

use crate::aws::SqsClient;
use crate::processing::ProcessingLayer;
use crate::secrets::AppSecretManager;
use crate::storage::StorageLayerFactory;
use crate::{
    events::EventPublisherFactory,
    search::SearchIndexFactory,
    services::files::presigned::{safe_complete_presigned, CompletePresigned},
};
use docbox_database::{
    models::{folder::Folder, presigned_upload_task::PresignedUploadTask, tenant::Tenant},
    DatabasePoolCache,
};
use std::sync::Arc;

mod noop;
mod sqs;

pub enum AppNotificationQueue {
    Sqs(sqs::SqsNotificationQueue),
    Noop(noop::NoopNotificationQueue),
}

impl AppNotificationQueue {
    pub fn create_noop() -> Self {
        AppNotificationQueue::Noop(noop::NoopNotificationQueue)
    }

    pub fn create_sqs(sqs_client: SqsClient, queue_url: String) -> Self {
        AppNotificationQueue::Sqs(sqs::SqsNotificationQueue::create(sqs_client, queue_url))
    }

    pub async fn next_message(&mut self) -> Option<NotificationQueueMessage> {
        match self {
            AppNotificationQueue::Sqs(queue) => queue.next_message().await,
            AppNotificationQueue::Noop(queue) => queue.next_message().await,
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

#[derive(Clone)]
pub struct NotificationQueueData {
    pub db_cache: Arc<DatabasePoolCache<AppSecretManager>>,
    pub search: SearchIndexFactory,
    pub storage: StorageLayerFactory,
    pub events: EventPublisherFactory,
    pub processing: ProcessingLayer,
}

/// Processes events coming from the notification queue. This will be
/// things like successful file uploads that need to be processed
pub async fn process_notification_queue(
    mut notification_queue: AppNotificationQueue,
    data: NotificationQueueData,
) {
    // Process messages from the notification queue
    while let Some(message) = notification_queue.next_message().await {
        match message {
            NotificationQueueMessage::FileCreated {
                bucket_name,
                object_key,
            } => {
                tokio::spawn(safe_handle_file_uploaded(
                    data.clone(),
                    bucket_name,
                    object_key,
                ));
            }
        }
    }
}

pub async fn safe_handle_file_uploaded(
    data: NotificationQueueData,
    bucket_name: String,
    object_key: String,
) {
    if let Err(cause) = handle_file_uploaded(data, bucket_name, object_key).await {
        tracing::error!(?cause, "failed to handle sqs file upload");
    }
}

pub async fn handle_file_uploaded(
    data: NotificationQueueData,
    bucket_name: String,
    object_key: String,
) -> anyhow::Result<()> {
    let tenant = {
        let db = data.db_cache.get_root_pool().await?;
        match Tenant::find_by_bucket(&db, &bucket_name).await? {
            Some(value) => value,
            None => {
                tracing::warn!(?bucket_name, ?object_key, "file was uploaded into a bucket sqs is listening to but there was no matching tenant");
                return Ok(());
            }
        }
    };

    let object_key = match urlencoding::decode(&object_key) {
        Ok(value) => value.to_string(),
        Err(err) => {
            tracing::warn!(
                ?err,
                ?bucket_name,
                ?object_key,
                "file was uploaded into a bucket but had an invalid file name"
            );
            return Ok(());
        }
    };

    let db = data.db_cache.get_tenant_pool(&tenant).await?;

    // Locate a pending upload task for the uploaded file
    let task = match PresignedUploadTask::find_by_file_key(&db, &object_key).await {
        Ok(Some(task)) => task,
        Ok(None) => {
            tracing::debug!("uploaded file was not a presigned upload");
            return Ok(());
        }
        Err(cause) => {
            tracing::error!(?cause, "unable to query presigned upload");
            anyhow::bail!("unable to query presigned upload");
        }
    };

    let scope = task.document_box.clone();

    // Retrieve the target folder
    let folder = match Folder::find_by_id(&db, &scope, task.folder_id).await {
        Ok(Some(value)) => value,
        Ok(None) => {
            tracing::error!("presigned upload folder no longer exists");
            anyhow::bail!("presigned upload folder no longer exists");
        }
        Err(cause) => {
            tracing::error!(?cause, "unable to query folder");
            anyhow::bail!("unable to query folder");
        }
    };

    // Update stored editing user data
    let complete = CompletePresigned { task, folder };

    let search = data.search.create_search_index(&tenant);
    let storage = data.storage.create_storage_layer(&tenant);
    let events = data.events.create_event_publisher(&tenant);

    // Create task future that performs the file upload
    safe_complete_presigned(db, search, storage, events, data.processing, complete).await?;

    Ok(())
}
