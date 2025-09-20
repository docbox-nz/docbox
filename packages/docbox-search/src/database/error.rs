use thiserror::Error;

#[derive(Debug, Error)]
pub enum DatabaseSearchIndexFactoryError {}

#[derive(Debug, Error)]
pub enum DatabaseSearchError {
    #[error("failed to connect")]
    AcquireDatabase,

    #[error("migration not found")]
    MigrationNotFound,

    #[error("failed to search index")]
    SearchIndex,

    #[error("failed to count page matches")]
    CountFilePages,

    #[error("failed to search file pages")]
    SearchFilePages,

    #[error("failed to delete search data")]
    DeleteData,

    #[error("failed to apply migration")]
    ApplyMigration,

    #[error("failed to add search data")]
    AddData,
}
