use std::path::PathBuf;

use docbox_database::{models::tenant::Tenant, ROOT_DATABASE_NAME};
use eyre::Context;
use uuid::Uuid;

use crate::{connect_db, CliConfiguration};

pub async fn migrate(
    config: &CliConfiguration,
    env: String,
    file: PathBuf,
    tenant_id: Option<Uuid>,
    skip_failed: bool,
) -> eyre::Result<()> {
    let root = match connect_db(
        &config.database.host,
        config.database.port,
        &config.database.username,
        &config.database.password,
        ROOT_DATABASE_NAME,
    )
    .await
    {
        Ok(value) => value,
        Err(err) => {
            eprintln!("failed to connect to root database: {err:?}");
            return Err(eyre::Error::msg("failed to connect to root database"));
        }
    };
    let tenants = Tenant::all(&root).await.context("failed to get tenants")?;

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

    let migration = tokio::fs::read_to_string(file)
        .await
        .context("failed to read migration file")?;

    for tenant in tenants {
        println!(
            "applying migration against {} ({} {:?})",
            tenant.id, tenant.db_name, tenant.env
        );

        let db = match connect_db(
            &config.database.host,
            config.database.port,
            &config.database.username,
            &config.database.password,
            &tenant.db_name,
        )
        .await
        {
            Ok(value) => value,
            Err(err) => {
                eprintln!("failed to connect to tenant database: {err:?}");
                println!("completed migrations: {}", applied_tenants.join(","));
                return Err(eyre::Error::msg("failed to connect to tenant database"));
            }
        };

        let result = match sqlx::raw_sql(&migration).execute(&db).await {
            Ok(value) => value,
            Err(cause) => {
                eprintln!("failed to apply migration to tenant database: {cause:?}");

                if skip_failed {
                    continue;
                }

                println!("completed migrations: {}", applied_tenants.join(","));
                return Err(eyre::Error::new(cause));
            }
        };

        println!(
            "applied migration against {} ({} {:?}) (rows affected: {})",
            tenant.id,
            tenant.db_name,
            tenant.env,
            result.rows_affected()
        );

        applied_tenants.push(tenant.id.to_string());
    }

    Ok(())
}
