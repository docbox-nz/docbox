use docbox_database::create::{create_database, create_tenants_table};
use eyre::Context;

use crate::{connect_db, Credentials};

/// Create and initialize the root docbox database
pub async fn create_root() -> eyre::Result<()> {
    let credentials_raw = tokio::fs::read("private/cli-credentials.json").await?;
    let credentials: Credentials = serde_json::from_slice(&credentials_raw)?;

    // Connect to the root postgres database
    let db_root = connect_db(
        &credentials.host,
        credentials.port,
        &credentials.username,
        &credentials.password,
        "postgres",
    )
    .await
    .context("failed to connect to postgres database")?;

    tracing::debug!(?credentials);

    // Create the tenant database
    if let Err(err) = create_database(&db_root, "docbox").await {
        if !err
            .as_database_error()
            .is_some_and(|err| err.code().is_some_and(|code| code.to_string().eq("42P04")))
        {
            return Err(err.into());
        }
    }

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

    // Initialize the docbox database
    create_tenants_table(&db_docbox)
        .await
        .context("failed to setup tenants table")?;

    Ok(())
}
