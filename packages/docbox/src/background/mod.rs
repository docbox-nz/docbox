use docbox_database::DatabasePoolCache;
use docbox_storage::StorageLayerFactory;
use futures::StreamExt;
use scheduler::{SchedulerEventStream, SchedulerQueueEvent};
use std::sync::Arc;

pub mod purge_expired_presigned_tasks;
pub mod purge_expired_website_metadata;
pub mod scheduler;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum BackgroundEvent {
    /// Task to purge presigned URLs
    PurgeExpiredPresigned,

    /// Task to purge expired website metadata
    PurgeExpiredWebsiteMetadata,
}

pub struct BackgroundTaskData {
    pub db_cache: Arc<DatabasePoolCache>,
    pub storage: StorageLayerFactory,
}

pub async fn perform_background_tasks(data: BackgroundTaskData) {
    let events = vec![
        SchedulerQueueEvent {
            event: BackgroundEvent::PurgeExpiredPresigned,
            interval: 60 * 60,
        },
        SchedulerQueueEvent {
            event: BackgroundEvent::PurgeExpiredWebsiteMetadata,
            interval: 60 * 60,
        },
    ];

    let mut events = SchedulerEventStream::new(events);

    while let Some(event) = events.next().await {
        match event {
            BackgroundEvent::PurgeExpiredPresigned => {
                tracing::debug!("performing background purge for presigned tasks");
                tokio::spawn(
                    purge_expired_presigned_tasks::safe_purge_expired_presigned_tasks(
                        data.db_cache.clone(),
                        data.storage.clone(),
                    ),
                );
            }
            BackgroundEvent::PurgeExpiredWebsiteMetadata => {
                tracing::debug!("purging expired website metadata");
                tokio::spawn(
                    purge_expired_website_metadata::safe_purge_expired_website_metadata(
                        data.db_cache.clone(),
                    ),
                );
            }
        }
    }
}
