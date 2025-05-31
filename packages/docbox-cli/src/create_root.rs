use docbox_database::{
    create::{create_database, create_tenants_table},
    ROOT_DATABASE_NAME,
};
use eyre::Context;

use crate::{connect_db, CliConfiguration};

/// Create and initialize the root docbox database
pub async fn create_root(config: &CliConfiguration) -> eyre::Result<()> {
    // Connect to the root postgres database
    let db_root = connect_db(
        &config.database.host,
        config.database.port,
        &config.database.username,
        &config.database.password,
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
        &config.database.username,
        &config.database.password,
        ROOT_DATABASE_NAME,
    )
    .await
    .context("failed to connect to docbox database")?;

    // Initialize the docbox database
    create_tenants_table(&db_docbox)
        .await
        .context("failed to setup tenants table")?;

    Ok(())
}
