use crate::search::{
    models::{SearchIndexData, SearchIndexType},
    TenantSearchIndex,
};
use anyhow::Context;
use docbox_database::{
    models::{document_box::DocumentBoxScope, link::Link, tenant::Tenant},
    DbPool,
};
use futures::{future::BoxFuture, stream::FuturesUnordered, StreamExt};

use super::create_link::CreateLinkError;

pub async fn store_link_index(
    search: &TenantSearchIndex,
    link: &Link,
    scope: &DocumentBoxScope,
) -> Result<(), CreateLinkError> {
    search
        .add_data(SearchIndexData {
            ty: SearchIndexType::Link,
            item_id: link.id,
            folder_id: link.folder_id,
            name: link.name.to_string(),
            mime: None,
            content: Some(link.value.clone()),
            pages: None,
            created_at: link.created_at.to_rfc3339(),
            created_by: link.created_by.clone(),
            document_box: scope.clone(),
        })
        .await
        .map_err(CreateLinkError::CreateIndex)?;

    Ok(())
}

pub async fn re_index_link(
    search: &TenantSearchIndex,
    scope: &DocumentBoxScope,
    link: Link,
) -> anyhow::Result<()> {
    store_link_index(search, &link, scope).await?;
    Ok(())
}

/// goes through all links the tenant and re-indexes them
pub async fn re_index_links(
    db: &DbPool,
    search: &TenantSearchIndex,
    tenant: &Tenant,
) -> anyhow::Result<()> {
    let tenant_id = tenant.id;

    let mut page_index = 0;
    const PAGE_SIZE: u64 = 500;

    loop {
        let links = Link::all(db, page_index * PAGE_SIZE, PAGE_SIZE)
            .await
            .with_context(|| format!("failed to load files page: {page_index}"))?;

        let is_end = (links.len() as u64) < PAGE_SIZE;

        // Apply migration to all tenants
        let results: Vec<anyhow::Result<()>> = links
            .into_iter()
            .map(|link| -> BoxFuture<'_, anyhow::Result<()>> {
                Box::pin(async {
                    let link = link;
                    let link_id = link.link.id;
                    re_index_link(search, &link.scope, link.link)
                        .await
                        .with_context(|| {
                            format!(
                                "failed to migrate file: Tenant ID: {tenant_id} Link ID: {link_id}"
                            )
                        })
                })
            })
            .collect::<FuturesUnordered<BoxFuture<'_, anyhow::Result<()>>>>()
            .collect()
            .await;

        for result in results {
            if let Err(err) = result {
                tracing::error!(?err, "failed to migrate tenant")
            }
        }

        if is_end {
            break;
        }

        page_index += 1;
    }

    Ok(())
}
