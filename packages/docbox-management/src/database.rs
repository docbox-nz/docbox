use docbox_database::{DbPool, DbResult};

/// Provider to get database access for the management tool
///
/// Expects that the access granted to the database is sufficient
/// for creation and deletion of tables
pub trait DatabaseProvider: Send + Sync + 'static {
    /// Connect to the provided `database` providing back a [DbPool]
    fn connect(&self, database: &str) -> impl Future<Output = DbResult<DbPool>> + Send;
}

pub struct CloseOnDrop(DbPool);

pub fn close_pool_on_drop(pool: &DbPool) -> CloseOnDrop {
    CloseOnDrop(pool.clone())
}

impl Drop for CloseOnDrop {
    fn drop(&mut self) {
        let pool = self.0.clone();

        tokio::spawn(async move {
            tracing::debug!("closing dropped pool");
            pool.close().await;
            tracing::debug!("closed dropped pool");
        });
    }
}
