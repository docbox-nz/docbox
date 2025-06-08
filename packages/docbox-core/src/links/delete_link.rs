use crate::events::{TenantEventMessage, TenantEventPublisher};
use docbox_database::{
    DbErr, DbPool,
    models::{
        document_box::{DocumentBoxScopeRaw, WithScope},
        link::Link,
    },
};
use docbox_search::TenantSearchIndex;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DeleteLinkError {
    #[error(transparent)]
    Database(#[from] DbErr),
    #[error(transparent)]
    Search(anyhow::Error),
}

#[tracing::instrument(skip_all, fields(%scope, link_id = %link.id))]
pub async fn delete_link(
    db: &DbPool,
    search: &TenantSearchIndex,
    events: &TenantEventPublisher,
    link: Link,
    scope: DocumentBoxScopeRaw,
) -> Result<(), DeleteLinkError> {
    // Delete the indexed file contents
    search
        .delete_data(link.id)
        .await
        .map_err(DeleteLinkError::Search)?;

    // Delete the link itself from the db
    let result = link
        .delete(db)
        .await
        .inspect_err(|error| tracing::error!(?error, "failed to delete link"))?;

    // Check we actually removed something before emitting an event
    if result.rows_affected() < 1 {
        return Ok(());
    }

    // Publish an event
    events.publish_event(TenantEventMessage::LinkDeleted(WithScope::new(link, scope)));

    Ok(())
}
