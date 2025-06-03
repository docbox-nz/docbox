use docbox_core::{aws::aws_config, secrets::AppSecretManager};
use docbox_database::{
    migrations::apply_tenant_migrations, models::tenant::Tenant, DatabasePoolCache, DbPool,
};
use eyre::Context;
use uuid::Uuid;

use crate::CliConfiguration;

pub async fn migrate(
    config: &CliConfiguration,
    env: String,
    tenant_id: Option<Uuid>,
    skip_failed: bool,
) -> eyre::Result<()> {
    // Load AWS configuration
    let aws_config = aws_config().await;

    // Connect to secrets manager
    let secrets = AppSecretManager::from_config(&aws_config, config.secrets.clone());

    // Setup database cache / connector
    let db_cache = DatabasePoolCache::new(
        config.database.host.clone(),
        config.database.port,
        config.database.root_secret_name.clone(),
        secrets,
    );
    let root_db = db_cache.get_root_pool().await?;

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
        let result = migrate_tenant(&db_cache, &root_db, &tenant).await;
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
    db_cache: &DatabasePoolCache<AppSecretManager>,
    root_db: &DbPool,
    tenant: &Tenant,
) -> eyre::Result<()> {
    tracing::debug!(
        tenant_id = ?tenant.id,
        tenant_env = ?tenant.env,
        "applying migration against",
    );

    let db = db_cache
        .get_tenant_pool(tenant)
        .await
        .context("failed to connect to tenant database")?;

    let mut root_t = root_db.begin().await?;
    let mut t = db.begin().await?;
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
