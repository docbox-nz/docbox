use docbox_core::{aws::aws_config, search::SearchIndexFactory, secrets::AppSecretManager};
use docbox_database::{models::tenant::Tenant, DatabasePoolCache};
use eyre::Context;
use uuid::Uuid;

use crate::{AnyhowError, CliConfiguration};

pub async fn migrate_search(
    config: &CliConfiguration,
    env: String,
    name: String,
    tenant_id: Option<Uuid>,
    skip_failed: bool,
) -> eyre::Result<()> {
    tracing::debug!(?env, ?tenant_id, "migrating tenant search");

    // Load AWS configuration
    let aws_config = aws_config().await;

    // Connect to secrets manager
    let secrets = AppSecretManager::from_config(&aws_config, config.secrets.clone());

    tracing::info!("created database secret");

    // Setup database cache / connector
    let db_cache = DatabasePoolCache::new(
        config.database.host.clone(),
        config.database.port,
        config.database.root_secret_name.clone(),
        secrets,
    );

    let search_factory =
        SearchIndexFactory::from_config(&aws_config, config.search.clone()).map_err(AnyhowError)?;

    let root_db = db_cache.get_root_pool().await?;

    let tenants = Tenant::all(&root_db)
        .await
        .context("failed to get tenants")?;

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
        println!(
            "applying migration against {} ({} {:?})",
            tenant.id, tenant.db_name, tenant.env
        );

        let search = search_factory.create_search_index(&tenant);
        if let Err(error) = search.apply_migration(&name).await {
            tracing::error!(?error, ?tenant, "failed to apply migration to tenant");
            if skip_failed {
                continue;
            }

            return Err(eyre::Error::new(AnyhowError(error)));
        }

        println!(
            "applied migration against {} ({} {:?})",
            tenant.id, tenant.db_name, tenant.env,
        );

        applied_tenants.push(tenant.id.to_string());
    }

    Ok(())
}
