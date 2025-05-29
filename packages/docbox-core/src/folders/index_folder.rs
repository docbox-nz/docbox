use crate::search::{
    models::{SearchIndexData, SearchIndexType},
    TenantSearchIndex,
};
use anyhow::Context;
use docbox_database::{
    models::{folder::Folder, tenant::Tenant},
    DbPool,
};
use futures::{future::BoxFuture, stream::FuturesUnordered, StreamExt};

pub async fn re_index_folder(search: &TenantSearchIndex, folder: Folder) -> anyhow::Result<()> {
    let folder_id = match folder.folder_id {
        Some(value) => value,
        // Root folders are not included in the index
        None => return Ok(()),
    };

    // Re-create base folder index
    search
        .add_data(SearchIndexData {
            ty: SearchIndexType::Folder,
            item_id: folder.id,
            folder_id,
            name: folder.name,
            mime: None,
            content: None,
            pages: None,
            created_at: folder.created_at.to_rfc3339(),
            created_by: folder.created_by.clone(),
            document_box: folder.document_box.clone(),
        })
        .await
        .context("failed to create file base index")?;

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
