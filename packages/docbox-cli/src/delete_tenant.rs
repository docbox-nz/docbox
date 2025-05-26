use docbox_database::models::tenant::{Tenant, TenantId};
use eyre::{Context, ContextCompat};

use crate::{connect_db, Credentials};

pub async fn delete_tenant(env: String, tenant_id: TenantId) -> eyre::Result<()> {
    // Load CLI credentials
    let credentials_raw = tokio::fs::read("private/cli-credentials.json").await?;
    let credentials: Credentials = serde_json::from_slice(&credentials_raw)?;

    // Connect to the docbox database
    let db_docbox = connect_db(
        &credentials.host,
        credentials.port,
        &credentials.username,
        &credentials.password,
        "docbox",
    )
    .await
    .context("failed to connect to docbox database")?;

    // Get the tenant details
    let tenant = Tenant::find_by_id(&db_docbox, tenant_id, &env)
        .await
        .context("failed to request tenant")?
        .context("tenant not found")?;
    tracing::debug!(?tenant, "found tenant");

    // ..TODO: Optionally delete S3 bucket, opensearch index, database

    tenant
        .delete(&db_docbox)
        .await
        .context("failed to delete tenant")?;

    tracing::info!("tenant created successfully");

    Ok(())
}
