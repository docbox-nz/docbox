use async_trait::async_trait;
use models::tenant::Tenant;
use moka::{future::Cache, policy::EvictionPolicy};
use serde::{Deserialize, Serialize};
pub use sqlx::postgres::PgSslMode;
pub use sqlx::{
    PgPool, Postgres, Transaction,
    postgres::{PgConnectOptions, PgPoolOptions},
};

use std::{error::Error, time::Duration};
use thiserror::Error;
use tracing::debug;

pub use sqlx;
pub use sqlx::PgExecutor as DbExecutor;

pub mod create;
pub mod migrations;
pub mod models;

/// Type of the database connection pool
pub type DbPool = PgPool;

/// Short type alias for a database error
pub type DbErr = sqlx::Error;

/// Type alias for a result where the error is a [DbErr]
pub type DbResult<T> = Result<T, DbErr>;

/// Type of a database transaction
pub type DbTransaction<'c> = Transaction<'c, Postgres>;

/// Duration to maintain database pool caches (48h)
const DB_CACHE_DURATION: Duration = Duration::from_secs(60 * 60 * 48);

/// Duration to cache database credentials for (12h)
const DB_CONNECT_INFO_CACHE_DURATION: Duration = Duration::from_secs(60 * 60 * 12);

/// Name of the root database
pub const ROOT_DATABASE_NAME: &str = "docbox";

/// Cache for database pools
pub struct DatabasePoolCache<S: DbSecretManager> {
    /// Database host
    host: String,

    /// Database port
    port: u16,

    /// Name of the secrets manager secret that contains
    /// the credentials for the root "docbox" database
    root_secret_name: String,

    /// Cache from the database name to the pool for that database
    cache: Cache<String, DbPool>,

    /// Cache for the connection info details, stores the last known
    /// credentials and the instant that they were obtained at
    connect_info_cache: Cache<String, DbSecrets>,

    /// Secrets manager access to load credentials
    secrets_manager: S,
}

/// Username and password for a specific database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbSecrets {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Error)]
pub enum DbConnectErr {
    #[error("database credentials not found in secrets manager")]
    MissingCredentials,

    #[error(transparent)]
    SecretsManager(Box<dyn Error + Send + Sync + 'static>),

    #[error(transparent)]
    Db(#[from] DbErr),
}

#[async_trait]
pub trait DbSecretManager: Send + Sync {
    async fn get_secret(&self, name: &str) -> Result<Option<DbSecrets>, DbConnectErr>;
}

impl<S> DatabasePoolCache<S>
where
    S: DbSecretManager,
{
    pub fn new(host: String, port: u16, root_secret_name: String, secrets_manager: S) -> Self {
        let cache = Cache::builder()
            .time_to_idle(DB_CACHE_DURATION)
            .max_capacity(50)
            .eviction_policy(EvictionPolicy::tiny_lfu())
            .build();

        let connect_info_cache = Cache::builder()
            .time_to_idle(DB_CONNECT_INFO_CACHE_DURATION)
            .max_capacity(50)
            .eviction_policy(EvictionPolicy::tiny_lfu())
            .build();

        Self {
            host,
            port,
            root_secret_name,
            cache,
            connect_info_cache,
            secrets_manager,
        }
    }

    /// Request a database pool for the root database
    pub async fn get_root_pool(&self) -> Result<PgPool, DbConnectErr> {
        self.get_pool(ROOT_DATABASE_NAME, &self.root_secret_name)
            .await
    }

    /// Request a database pool for a specific tenant
    pub async fn get_tenant_pool(&self, tenant: &Tenant) -> Result<PgPool, DbConnectErr> {
        self.get_pool(&tenant.db_name, &tenant.db_secret_name).await
    }

    /// Empties all the caches
    pub async fn flush(&self) {
        // Clear cache
        self.cache.invalidate_all();
        self.connect_info_cache.invalidate_all();
    }

    /// Obtains a database pool connection to the database with the provided name
    async fn get_pool(&self, db_name: &str, secret_name: &str) -> Result<PgPool, DbConnectErr> {
        let cache_key = format!("{}-{}", db_name, secret_name);

        if let Some(pool) = self.cache.get(&cache_key).await {
            return Ok(pool);
        }

        let pool = self.create_pool(db_name, secret_name).await?;
        self.cache.insert(cache_key, pool.clone()).await;

        Ok(pool)
    }

    /// Obtains database connection info
    async fn get_credentials(&self, secret_name: &str) -> Result<DbSecrets, DbConnectErr> {
        if let Some(connect_info) = self.connect_info_cache.get(secret_name).await {
            return Ok(connect_info);
        }

        // Load new credentials
        let credentials = self
            .secrets_manager
            .get_secret(secret_name)
            .await
            .map_err(|err| DbConnectErr::SecretsManager(err.into()))?
            .ok_or(DbConnectErr::MissingCredentials)?;

        // Cache the credential
        self.connect_info_cache
            .insert(secret_name.to_string(), credentials.clone())
            .await;

        Ok(credentials)
    }

    /// Creates a database pool connection
    async fn create_pool(&self, db_name: &str, secret_name: &str) -> Result<PgPool, DbConnectErr> {
        debug!(?db_name, ?secret_name, "creating db pool connection");

        let credentials = self.get_credentials(secret_name).await?;
        let options = PgConnectOptions::new()
            .host(&self.host)
            .port(self.port)
            .username(&credentials.username)
            .password(&credentials.password)
            .database(db_name);

        match PgPoolOptions::new().connect_with(options).await {
            // Success case
            Ok(value) => Ok(value),
            Err(err) => {
                // Drop the connect info cache incase the credentials were wrong
                self.connect_info_cache.remove(secret_name).await;
                Err(DbConnectErr::Db(err))
            }
        }
    }
}
