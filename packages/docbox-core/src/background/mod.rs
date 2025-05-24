use crate::{
    secrets::AppSecretManager, services::files::presigned::safe_purge_expired_presigned_tasks,
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

pub async fn perform_background_tasks(
    db_cache: Arc<DatabasePoolCache<AppSecretManager>>,
    storage: StorageLayerFactory,
) {
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
                    db_cache.clone(),
                    storage.clone(),
                ));
            }
        }
    }
}
