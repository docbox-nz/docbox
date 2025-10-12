//! # Processing
//!
//! Logic for processing notifications from the notification queue

use crate::notifications::{AppNotificationQueue, NotificationQueueMessage};
use crate::{
    events::EventPublisherFactory,
    files::upload_file_presigned::{CompletePresigned, safe_complete_presigned},
};
use docbox_database::{
    DatabasePoolCache,
    models::{folder::Folder, presigned_upload_task::PresignedUploadTask, tenant::Tenant},
};
use docbox_processing::ProcessingLayer;
use docbox_search::SearchIndexFactory;
use docbox_storage::StorageLayerFactory;
use std::sync::Arc;
use tracing::Instrument;

#[derive(Clone)]
pub struct NotificationQueueData {
    pub db_cache: Arc<DatabasePoolCache>,
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
                tokio::spawn(handle_file_uploaded(data.clone(), bucket_name, object_key));
            }
        }
    }
}

/// Handle file upload notifications
#[tracing::instrument(skip(data))]
pub async fn handle_file_uploaded(
    data: NotificationQueueData,
    bucket_name: String,
    object_key: String,
) {
    let tenant = {
        let db = match data.db_cache.get_root_pool().await {
            Ok(value) => value,
            Err(error) => {
                tracing::error!(?error, "failed to acquire root database pool");
                return;
            }
        };

        match Tenant::find_by_bucket(&db, &bucket_name).await {
            Ok(Some(value)) => value,
            Ok(None) => {
                tracing::warn!(
                    "file was uploaded into a bucket sqs is listening to but there was no matching tenant"
                );
                return;
            }
            Err(error) => {
                tracing::error!(?error, "failed to query tenant for bucket");
                return;
            }
        }
    };

    // Provide a span that contains the tenant metadata
    let span = tracing::info_span!("tenant", tenant_id = %tenant.id, tenant_env = %tenant.env);

    handle_file_uploaded_tenant(tenant, data, bucket_name, object_key)
        .instrument(span)
        .await;
}

/// Handle file upload notification once the tenant has been identified
#[tracing::instrument(skip(data))]
pub async fn handle_file_uploaded_tenant(
    tenant: Tenant,
    data: NotificationQueueData,
    bucket_name: String,
    object_key: String,
) {
    let object_key = match urlencoding::decode(&object_key) {
        Ok(value) => value.to_string(),
        Err(error) => {
            tracing::warn!(
                ?error,
                "file was uploaded into a bucket but had an invalid file name"
            );
            return;
        }
    };

    let db = match data.db_cache.get_tenant_pool(&tenant).await {
        Ok(value) => value,
        Err(error) => {
            tracing::error!(?error, "failed to get tenant database pool");
            return;
        }
    };

    // Locate a pending upload task for the uploaded file
    let task = match PresignedUploadTask::find_by_file_key(&db, &object_key).await {
        Ok(Some(task)) => task,
        // Ignore files that aren't attached to a presigned upload task
        // (Things like generated files will show up here)
        Ok(None) => {
            tracing::debug!("uploaded file was not a presigned upload");
            return;
        }
        Err(error) => {
            tracing::error!(?error, "unable to query presigned upload");
            return;
        }
    };

    let scope = task.document_box.clone();

    // Retrieve the target folder
    let folder = match Folder::find_by_id(&db, &scope, task.folder_id).await {
        Ok(Some(value)) => value,
        Ok(None) => {
            tracing::error!("presigned upload folder no longer exists");
            return;
        }
        Err(error) => {
            tracing::error!(?error, "unable to query folder");
            return;
        }
    };

    // Update stored editing user data
    let complete = CompletePresigned { task, folder };

    let search = data.search.create_search_index(&tenant);
    let storage = data.storage.create_storage_layer(&tenant);
    let events = data.events.create_event_publisher(&tenant);

    // Create task future that performs the file upload
    if let Err(error) =
        safe_complete_presigned(db, search, storage, events, data.processing, complete).await
    {
        tracing::error!(?error, "failed to complete presigned file upload");
    }
}
