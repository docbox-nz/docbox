use docbox_database::{DbConnectErr, DbErr};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DatabaseSearchIndexFactoryError {}

#[derive(Debug, Error)]
pub enum DatabaseSearchError {
    #[error("failed to connect")]
    AcquireDatabase(DbConnectErr),

    #[error("migration not found")]
    MigrationNotFound,

    #[error("failed to search index")]
    SearchIndex(DbErr),

    #[error("failed to search file pages")]
    SearchFilePages,

    #[error("failed to delete search data")]
    DeleteData(DbErr),

    #[error("failed to apply migration")]
    ApplyMigration(DbErr),

    #[error("failed to add search data")]
    AddData(DbErr),
}
