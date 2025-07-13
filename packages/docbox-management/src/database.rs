use docbox_database::{DbPool, DbResult};

/// Provider to get database access for the management tool
///
/// Expects that the access granted to the database is sufficient
/// for creation and deletion of tables
pub trait DatabaseProvider: Send + Sync + 'static {
    /// Connect to the provided `database` providing back a [DbPool]
    fn connect(&self, database: &str) -> impl Future<Output = DbResult<DbPool>> + Send;
}
