use docbox_database::{
    DbPool, ROOT_DATABASE_NAME,
    migrations::apply_tenant_migrations,
    models::tenant::{Tenant, TenantId},
};
use eyre::Context;

use crate::{CliConfiguration, connect_db};

pub async fn migrate(
    config: &CliConfiguration,
    env: String,
    tenant_id: Option<TenantId>,
    skip_failed: bool,
) -> eyre::Result<()> {
    // Connect to the root database
    let root_db = connect_db(
        &config.database.host,
        config.database.port,
        &config.database.setup_user.username,
        &config.database.setup_user.password,
        ROOT_DATABASE_NAME,
    )
    .await
    .context("failed to connect to root database")?;

    // Load tenants from the database
    let tenants = Tenant::all(&root_db)
        .await
        .context("failed to get tenants")?;

    // Filter to our desired tenants
    let tenants: Vec<Tenant> = tenants
        .into_iter()
        .filter(|tenant| {
            if tenant.env != env {
                return false;
            }

            if tenant_id
                .as_ref()
                .is_some_and(|schema| tenant.id.ne(schema))
            {
                return false;
            }

            true
        })
        .collect();

    let mut applied_tenants = Vec::new();

    for tenant in tenants {
        let result = migrate_tenant(config, &root_db, &tenant).await;
        match result {
            Ok(_) => {
                applied_tenants.push((tenant.env, tenant.id));
            }
            Err(error) => {
                tracing::error!(?error, "failed to connect to tenant database");
                if !skip_failed {
                    tracing::debug!(?applied_tenants, "completed migrations");
                    break;
                }
            }
        }
    }

    tracing::debug!(?applied_tenants, "completed migrations");

    Ok(())
}

pub async fn migrate_tenant(
    config: &CliConfiguration,
    root_db: &DbPool,
    tenant: &Tenant,
) -> eyre::Result<()> {
    tracing::debug!(
        tenant_id = ?tenant.id,
        tenant_env = ?tenant.env,
        "applying migration against",
    );

    // Connect to the tenant database
    let tenant_db = connect_db(
        &config.database.host,
        config.database.port,
        &config.database.setup_user.username,
        &config.database.setup_user.password,
        &tenant.db_name,
    )
    .await
    .context("failed to connect to tenant database")?;

    let mut root_t = root_db.begin().await?;
    let mut t = tenant_db.begin().await?;
    apply_tenant_migrations(&mut root_t, &mut t, tenant, None).await?;
    t.commit().await?;
    root_t.commit().await?;

    tracing::info!(
        tenant_id = ?tenant.id,
        tenant_env = ?tenant.env,
        "applied migrations against",
    );

    Ok(())
}
