use crate::{
    files::purge_expired_presigned_tasks::safe_purge_expired_presigned_tasks,
    storage::StorageLayerFactory,
};
use docbox_database::DatabasePoolCache;
use futures::StreamExt;
use scheduler::{SchedulerEventStream, SchedulerQueueEvent};
use std::sync::Arc;

pub mod scheduler;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum BackgroundEvent {
    /// Task to purge presigned URLs
    PurgeExpiredPresigned,
}

pub struct BackgroundTaskData {
    pub db_cache: Arc<DatabasePoolCache>,
    pub storage: StorageLayerFactory,
}

pub async fn perform_background_tasks(data: BackgroundTaskData) {
    let events = vec![SchedulerQueueEvent {
        event: BackgroundEvent::PurgeExpiredPresigned,
        interval: 60 * 60,
    }];

    let mut events = SchedulerEventStream::new(events);

    while let Some(event) = events.next().await {
        match event {
            BackgroundEvent::PurgeExpiredPresigned => {
                tracing::debug!("performing background purge for presigned tasks");
                tokio::spawn(safe_purge_expired_presigned_tasks(
                    data.db_cache.clone(),
                    data.storage.clone(),
                ));
            }
        }
    }
}
