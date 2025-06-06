use docbox_core::{aws::aws_config, secrets::AppSecretManager};
use docbox_database::{
    ROOT_DATABASE_NAME,
    create::{create_database, create_restricted_role, create_tenants_table},
};
use eyre::Context;
use serde_json::json;

use crate::{CliConfiguration, connect_db};

/// Create and initialize the root docbox database
pub async fn create_root(config: &CliConfiguration) -> eyre::Result<()> {
    // Load AWS configuration
    let aws_config = aws_config().await;
    let secrets = AppSecretManager::from_config(&aws_config, config.secrets.clone());

    // Connect to the root postgres database
    let db_root = connect_db(
        &config.database.host,
        config.database.port,
        &config.database.setup_user.username,
        &config.database.setup_user.password,
        "postgres",
    )
    .await
    .context("failed to connect to postgres database")?;

    // Create the tenant database
    if let Err(err) = create_database(&db_root, ROOT_DATABASE_NAME).await {
        if !err
            .as_database_error()
            .is_some_and(|err| err.code().is_some_and(|code| code.to_string().eq("42P04")))
        {
            return Err(err.into());
        }
    }

    // Connect to the docbox database
    let db_docbox = connect_db(
        &config.database.host,
        config.database.port,
        &config.database.setup_user.username,
        &config.database.setup_user.password,
        ROOT_DATABASE_NAME,
    )
    .await
    .context("failed to connect to docbox database")?;

    // Setup the restricted root db role
    create_restricted_role(
        &db_docbox,
        ROOT_DATABASE_NAME,
        &config.database.root_role_name,
        &config.database.root_secret_password,
    )
    .await
    .context("failed to setup root user")?;
    tracing::info!("created root user");

    let secret_value = serde_json::to_string(&json!({
        "username": config.database.root_role_name,
        "password": config.database.root_secret_password
    }))?;

    secrets
        .create_secret(&config.database.root_secret_name, &secret_value)
        .await
        .map_err(|err| eyre::Error::msg(err.to_string()))?;

    tracing::info!("created database secret");

    // Initialize the docbox database
    create_tenants_table(&db_docbox)
        .await
        .context("failed to setup tenants table")?;

    Ok(())
}
