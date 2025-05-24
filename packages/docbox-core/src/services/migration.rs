use crate::{
    search::TenantSearchIndex,
    services::{folders::re_index_folder, links::re_index_link},
};
use anyhow::Context;
use docbox_database::{
    models::{folder::Folder, link::Link, tenant::Tenant},
    DbPool,
};
use futures::{future::BoxFuture, stream::FuturesUnordered, StreamExt};
use tracing::error;

/// goes through all links the tenant and re-indexes them
#[allow(unused)]
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
                error!(?err, "failed to migrate tenant")
            }
        }

        if is_end {
            break;
        }

        page_index += 1;
    }

    Ok(())
}

/// goes through all folders the tenant and re-indexes them
#[allow(unused)]
pub async fn re_index_folders(
    db: &DbPool,
    search: &TenantSearchIndex,
    tenant: &Tenant,
) -> anyhow::Result<()> {
    let tenant_id = tenant.id;

    let mut page_index = 0;
    const PAGE_SIZE: u64 = 500;

    loop {
        let folders = Folder::all(db, page_index * PAGE_SIZE, PAGE_SIZE)
            .await
            .with_context(|| format!("failed to load folders page: {page_index}"))?;

        let is_end = (folders.len() as u64) < PAGE_SIZE;

        // Apply migration to all tenants
        let results: Vec<anyhow::Result<()>> = folders
            .into_iter()
            .map(|folder| -> BoxFuture<'_, anyhow::Result<()>> {
                Box::pin(async {
                    let folder_id = folder.id;
                    re_index_folder(search, folder).await.with_context(|| {
                        format!(
                            "failed to migrate folder: Tenant ID: {tenant_id} File ID: {folder_id}"
                        )
                    })
                })
            })
            .collect::<FuturesUnordered<BoxFuture<'_, anyhow::Result<()>>>>()
            .collect()
            .await;

        for result in results {
            if let Err(err) = result {
                error!(?err, "failed to migrate tenant")
            }
        }

        if is_end {
            break;
        }

        page_index += 1;
    }

    Ok(())
}
