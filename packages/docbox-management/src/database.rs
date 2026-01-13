/// docbox-database re-exports
pub use docbox_database::*;

use crate::config::AdminDatabaseConfiguration;

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

pub struct ServerDatabaseProvider {
    pub config: AdminDatabaseConfiguration,
    pub username: String,
    pub password: String,
}

impl DatabaseProvider for ServerDatabaseProvider {
    fn connect(&self, database: &str) -> impl Future<Output = DbResult<DbPool>> + Send {
        let options = PgConnectOptions::new()
            .host(&self.config.host)
            .port(self.config.port)
            .username(&self.username)
            .password(&self.password)
            .database(database);

        PgPool::connect_with(options)
    }
}
