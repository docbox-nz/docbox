use crate::{
    events::{TenantEventMessage, TenantEventPublisher},
    search::TenantSearchIndex,
};
use docbox_database::{
    models::{
        document_box::{DocumentBoxScope, WithScope},
        link::Link,
    },
    DbPool,
};

pub async fn delete_link(
    db: &DbPool,
    search: &TenantSearchIndex,
    events: &TenantEventPublisher,
    link: Link,
    scope: DocumentBoxScope,
) -> anyhow::Result<()> {
    // Delete the indexed file contents
    search.delete_data(link.id).await?;

    let link_id = link.id;

    // Delete the link itself from the db
    if let Err(cause) = link.delete(db).await {
        tracing::error!(?cause, ?link_id, "failed to delete link");
        anyhow::bail!("failed to delete link");
    }

    // Publish an event
    events.publish_event(TenantEventMessage::LinkDeleted(WithScope::new(link, scope)));

    Ok(())
}
