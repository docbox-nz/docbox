use docbox_database::models::tenant::{Tenant, TenantId};
use eyre::{Context, ContextCompat};

use crate::{connect_db, CliConfiguration};

pub async fn get_tenant(
    config: &CliConfiguration,
    env: String,
    tenant_id: TenantId,
) -> eyre::Result<()> {
    // Connect to the docbox database
    let db_docbox = connect_db(
        &config.database.host,
        config.database.port,
        &config.database.username,
        &config.database.password,
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

    Ok(())
}
