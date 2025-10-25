#![forbid(unsafe_code)]

// Pool re-exports
pub use pool::{
    DatabasePoolCache, DatabasePoolCacheConfig, DatabasePoolCacheConfigError, DbConnectErr,
    DbSecrets,
};

/// SQLx re-exports for other projects
pub use sqlx::{
    self, PgExecutor as DbExecutor, PgPool, Postgres, Transaction,
    postgres::{PgConnectOptions, PgPoolOptions},
};

pub mod create;
pub mod migrations;
pub mod models;
pub mod pool;
pub mod utils;

/// Type of the database connection pool
pub type DbPool = PgPool;

/// Short type alias for a database error
pub type DbErr = sqlx::Error;

/// Type alias for a result where the error is a [DbErr]
pub type DbResult<T> = Result<T, DbErr>;

/// Type of a database transaction
pub type DbTransaction<'c> = Transaction<'c, Postgres>;

/// Name of the root database
pub const ROOT_DATABASE_NAME: &str = "docbox";
