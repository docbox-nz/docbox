use crate::{
    database::{DatabaseProvider, close_pool_on_drop},
    password::random_password,
    root::migrate_root::{MigrateRootError, migrate_root},
};
use docbox_database::{
    DbErr, DbPool, DbResult, ROOT_DATABASE_NAME,
    create::{create_database, create_restricted_role},
    models::tenant::Tenant,
    sqlx::types::Uuid,
    utils::DatabaseErrorExt,
};
use docbox_secrets::{SecretManager, SecretManagerError};
use serde_json::json;
use thiserror::Error;

/// Temporary database to connect to while setting up the other databases
const TEMP_SETUP_DATABASE: &str = "postgres";

#[derive(Debug, Error)]
pub enum InitializeError {
    #[error("error connecting to 'postgres' database: {0}")]
    ConnectPostgres(DbErr),

    #[error("error creating root database: {0}")]
    CreateRootDatabase(DbErr),

    #[error("error connecting to root database: {0}")]
    ConnectRootDatabase(DbErr),

    #[error("error migrating root database: {0}")]
    MigrateRoot(MigrateRootError),

    #[error("error creating root database role: {0}")]
    CreateRootRole(DbErr),

    #[error("error serializing root secret: {0}")]
    SerializeSecret(serde_json::Error),

    #[error("failed to create root secret: {0}")]
    CreateRootSecret(SecretManagerError),

    #[error("error creating tenants table: {0}")]
    CreateTenantsTable(DbErr),
}

/// Check if the root database is initialized
#[tracing::instrument(skip(db_provider))]
pub async fn is_initialized(db_provider: &impl DatabaseProvider) -> DbResult<bool> {
    // First check if the root database exists
    let db = match db_provider.connect(ROOT_DATABASE_NAME).await {
        Ok(value) => value,
        Err(error) => {
            if error.is_database_does_not_exist() {
                // Database is not setup, server is not initialized
                return Ok(false);
            }

            return Err(error);
        }
    };

    tracing::debug!("root is initialized");

    let _guard = close_pool_on_drop(&db);

    // Then query the table for a non-existent tenant to make sure its setup correctly
    if let Err(error) = Tenant::find_by_id(&db, Uuid::nil(), "__DO_NOT_USE").await {
        if error.is_table_does_not_exist() {
            // Database is not setup, server is not initialized
            return Ok(false);
        }

        return Err(error);
    }

    tracing::debug!("tenant table is setup");

    Ok(true)
}

/// Initializes the root database of provida
#[tracing::instrument(skip(db_provider, secrets))]
pub async fn initialize(
    db_provider: &impl DatabaseProvider,
    secrets: &SecretManager,
    root_secret_name: &str,
) -> Result<(), InitializeError> {
    let db_docbox = initialize_root_database(db_provider).await?;
    let _guard = close_pool_on_drop(&db_docbox);

    let root_role_name = "docbox_config_api";
    let root_password = random_password(30);

    // Setup the restricted root db role
    initialize_root_role(&db_docbox, root_role_name, &root_password).await?;
    tracing::info!("created root user");

    // Setup the secret to store the role credentials
    initialize_root_secret(secrets, root_secret_name, root_role_name, &root_password).await?;
    tracing::info!("created database secret");

    // Migrate the root database
    migrate_root(db_provider, None)
        .await
        .map_err(InitializeError::MigrateRoot)?;

    Ok(())
}

/// Initializes the root database used by docbox
#[tracing::instrument(skip(db_provider))]
pub async fn initialize_root_database(
    db_provider: &impl DatabaseProvider,
) -> Result<DbPool, InitializeError> {
    // Connect to the root postgres database
    let db_root = db_provider
        .connect(TEMP_SETUP_DATABASE)
        .await
        .map_err(InitializeError::ConnectPostgres)?;

    let _guard = close_pool_on_drop(&db_root);

    // Create the tenant database
    if let Err(err) = create_database(&db_root, ROOT_DATABASE_NAME).await
        && !err
            .as_database_error()
            .is_some_and(|err| err.code().is_some_and(|code| code.to_string().eq("42P04")))
    {
        return Err(InitializeError::CreateRootDatabase(err));
    }

    // Connect to the docbox database
    let db_docbox = db_provider
        .connect(ROOT_DATABASE_NAME)
        .await
        .map_err(InitializeError::ConnectRootDatabase)?;

    Ok(db_docbox)
}

/// Initializes a root role that the docbox API will use when accessing
/// the tenants table
#[tracing::instrument(skip(db, root_role_password))]
pub async fn initialize_root_role(
    db: &DbPool,
    root_role_name: &str,
    root_role_password: &str,
) -> Result<(), InitializeError> {
    // Setup the restricted root db role
    create_restricted_role(db, ROOT_DATABASE_NAME, root_role_name, root_role_password)
        .await
        .map_err(InitializeError::CreateRootRole)?;

    Ok(())
}

/// Initializes and stores the secret for the root database access
#[tracing::instrument(skip(secrets, root_role_password))]
pub async fn initialize_root_secret(
    secrets: &SecretManager,
    root_secret_name: &str,
    root_role_name: &str,
    root_role_password: &str,
) -> Result<(), InitializeError> {
    let secret_value = serde_json::to_string(&json!({
        "username": root_role_name,
        "password": root_role_password
    }))
    .map_err(InitializeError::SerializeSecret)?;

    secrets
        .set_secret(root_secret_name, &secret_value)
        .await
        .map_err(InitializeError::CreateRootSecret)?;

    Ok(())
}
