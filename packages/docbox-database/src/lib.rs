use std::time::Duration;

use anyhow::Context;
use async_trait::async_trait;
use models::tenant::Tenant;
use moka::{future::Cache, policy::EvictionPolicy};
use serde::{Deserialize, Serialize};
use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions},
    PgPool, Postgres, Transaction,
};
use tracing::debug;

pub use sqlx::PgExecutor as DbExecutor;

pub mod create;
pub mod models;
pub mod setup;

/// Type of the database connection pool
pub type DbPool = PgPool;

/// Short type alias for a database error
pub type DbErr = sqlx::Error;

/// Type alias for a result where the error is a [DbErr]
pub type DbResult<T> = Result<T, DbErr>;

/// Type of a database transaction
pub type DbTransaction<'c> = Transaction<'c, Postgres>;

/// Info for connecting to databases
#[derive(Clone, Debug)]
pub struct DbConnectInfo {
    host: String,
    port: u16,
    user: String,
    password: String,
}

#[derive(Serialize, Deserialize)]
pub struct DbSecrets {
    pub username: String,
    pub password: String,
}

impl DbConnectInfo {
    pub async fn load<S>(secrets: &S, secret_name: &str) -> anyhow::Result<DbConnectInfo>
    where
        S: DbSecretManager,
    {
        let host =
            std::env::var("POSTGRES_HOST").context("missing environment variable POSTGRES_HOST")?;
        let port = std::env::var("POSTGRES_PORT")
            .context("missing environment variable POSTGRES_PORT")?
            .parse()
            .context("invalid POSTGRES_PORT port value")?;

        let DbSecrets { username, password } = secrets.get_secret(secret_name).await?;

        debug!("loaded database credentials from secrets");

        Ok(DbConnectInfo {
            host,
            port,
            user: username,
            password,
        })
    }
}

/// Duration to maintain database pool caches (48h)
const DB_CACHE_DURATION: Duration = Duration::from_secs(60 * 60 * 48);

/// Duration to cache database credentials for (12h)
const DB_CONNECT_INFO_CACHE_DURATION: Duration = Duration::from_secs(60 * 60 * 12);

/// Cache for database pools
pub struct DatabasePoolCache<S: DbSecretManager> {
    /// Cache from the database name to the pool for that database
    cache: Cache<String, DbPool>,

    /// Cache for the connection info details, stores the last known
    /// credentials and the instant that they were obtained at
    connect_info_cache: Cache<String, DbConnectInfo>,

    /// Secrets manager access to load credentials
    secrets_manager: S,
}

#[async_trait]
pub trait DbSecretManager: Send + Sync {
    async fn get_secret(&self, name: &str) -> anyhow::Result<DbSecrets>;
}

impl<S> DatabasePoolCache<S>
where
    S: DbSecretManager,
{
    pub fn new(secrets_manager: S) -> Self {
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
            cache,
            connect_info_cache,
            secrets_manager,
        }
    }

    /// Empties all the caches
    pub async fn flush(&self) {
        // Clear cache
        self.cache.invalidate_all();
        self.connect_info_cache.invalidate_all();
    }

    /// Obtains a database pool connection to the database with the provided name
    pub async fn get_pool(&self, db_name: &str, secret_name: &str) -> anyhow::Result<PgPool> {
        let cache_key = format!("{}-{}", db_name, secret_name);

        if let Some(pool) = self.cache.get(&cache_key).await {
            return Ok(pool);
        }

        let pool = self.create_pool(db_name, secret_name).await?;
        self.cache.insert(cache_key, pool.clone()).await;

        Ok(pool)
    }

    /// Obtains database connection info
    async fn get_connect_info(&self, secret_name: &str) -> anyhow::Result<DbConnectInfo> {
        if let Some(connect_info) = self.connect_info_cache.get(secret_name).await {
            return Ok(connect_info);
        }

        // Load new credentials
        let connect_info = DbConnectInfo::load(&self.secrets_manager, secret_name)
            .await
            .context("failed to load database credentials")?;

        // Cache the credential
        self.connect_info_cache
            .insert(secret_name.to_string(), connect_info.clone())
            .await;

        Ok(connect_info)
    }

    /// Creates a database pool connection
    async fn create_pool(&self, db_name: &str, secret_name: &str) -> anyhow::Result<PgPool> {
        debug!(?db_name, ?secret_name, "creating db pool connection");

        let connect_info = self.get_connect_info(secret_name).await?;
        let options = PgConnectOptions::new()
            .host(&connect_info.host)
            .port(connect_info.port)
            .username(&connect_info.user)
            .password(&connect_info.password)
            .database(db_name);

        match PgPoolOptions::new().connect_with(options).await {
            // Success case
            Ok(value) => Ok(value),
            Err(err) => {
                // Drop the connect info cache incase the credentials were wrong
                self.connect_info_cache.remove(secret_name).await;
                Err(anyhow::Error::new(err).context("failed to connect to database"))
            }
        }
    }
}

pub const ROOT_DATABASE_NAME: &str = "docbox";

/// Get a [PgPool] to the root docbox database. This is the "docbox" database, which contains
/// the tenants table.
pub async fn connect_root_database<S>(cache: &DatabasePoolCache<S>) -> anyhow::Result<PgPool>
where
    S: DbSecretManager,
{
    let root_credential = std::env::var("DOCBOX_DB_CREDENTIAL_NAME")
        .context("missing environment variable DOCBOX_DB_CREDENTIAL_NAME")?;

    cache.get_pool(ROOT_DATABASE_NAME, &root_credential).await
}

/// Get a [PgPool] database connection for a specific tenant database
pub async fn connect_tenant_database<S>(
    cache: &DatabasePoolCache<S>,
    tenant: &Tenant,
) -> anyhow::Result<DbPool>
where
    S: DbSecretManager,
{
    cache
        .get_pool(&tenant.db_name, &tenant.db_secret_name)
        .await
}
