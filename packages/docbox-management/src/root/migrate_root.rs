use crate::database::{DatabaseProvider, close_pool_on_drop};
use docbox_core::database::{
    DbErr, ROOT_DATABASE_NAME,
    create::check_database_table_exists,
    migrations::{apply_root_migrations, initialize_root_migrations},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MigrateRootError {
    #[error("error connecting to root database: {0}")]
    ConnectRootDatabase(DbErr),

    #[error("failed to check migrations table: {0}")]
    CheckMigrationTable(DbErr),

    #[error("failed to initialize migrations table: {0}")]
    CreateMigrationTable(DbErr),

    #[error("failed to apply migrations: {0}")]
    ApplyMigration(DbErr),

    #[error(transparent)]
    StartTransaction(DbErr),

    #[error(transparent)]
    CommitTransaction(DbErr),
}

#[tracing::instrument(skip(db_provider))]
pub async fn migrate_root(
    db_provider: &impl DatabaseProvider,
    target_migration_name: Option<&str>,
) -> Result<(), MigrateRootError> {
    // Connect to the root database
    let root_db = db_provider
        .connect(ROOT_DATABASE_NAME)
        .await
        .map_err(MigrateRootError::ConnectRootDatabase)?;

    let _guard = close_pool_on_drop(&root_db);

    // Check if the migrations table has been initialized
    // (Table did not exist before v0.4.0)
    if !check_database_table_exists(&root_db, "docbox_root_migrations")
        .await
        .map_err(MigrateRootError::CheckMigrationTable)?
    {
        // Initialize the migrations table
        initialize_root_migrations(&root_db)
            .await
            .map_err(MigrateRootError::CreateMigrationTable)?;
    }

    // Start transactions
    let mut root_t = root_db
        .begin()
        .await
        .map_err(MigrateRootError::StartTransaction)?;

    // Apply migrations
    apply_root_migrations(&mut root_t, target_migration_name)
        .await
        .map_err(MigrateRootError::ApplyMigration)?;

    // Commit database transaction
    root_t
        .commit()
        .await
        .map_err(MigrateRootError::CommitTransaction)?;

    Ok(())
}
