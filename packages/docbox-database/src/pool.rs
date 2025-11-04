//! # Database Pool
//!
//! This is the docbox solution for managing multiple database connections
//! and connection pools for each tenant and the root database itself.
//!
//! Pools are held in a cache with an expiry time to ensure they don't
//! hog too many database connections.
//!
//! Database pools and credentials are stored in a Tiny LFU cache these caches
//! can be flushed using [DatabasePoolCache::flush]
//!
//! ## Environment Variables
//!
//! * `DOCBOX_DB_HOST` - Database host
//! * `DOCBOX_DB_PORT` - Database port
//! * `DOCBOX_DB_CREDENTIAL_NAME` - Secrets manager name for the root database secret
//! * `DOCBOX_DB_MAX_CONNECTIONS` - Max connections each tenant pool can contain
//! * `DOCBOX_DB_MAX_ROOT_CONNECTIONS` - Max connections the root "docbox" pool can contain
//! * `DOCBOX_DB_ACQUIRE_TIMEOUT` - Timeout before acquiring a connection fails
//! * `DOCBOX_DB_IDLE_TIMEOUT` - Timeout before a idle connection is closed to save resources
//! * `DOCBOX_DB_CACHE_DURATION` - Duration idle pools should be maintained for before closing
//! * `DOCBOX_DB_CACHE_CAPACITY` - Maximum database pools to hold at once
//! * `DOCBOX_DB_CREDENTIALS_CACHE_DURATION` - Duration database credentials should be cached for
//! * `DOCBOX_DB_CREDENTIALS_CACHE_CAPACITY` - Maximum database credentials to cache

use crate::{DbErr, DbPool, ROOT_DATABASE_NAME, models::tenant::Tenant};
use docbox_secrets::{SecretManager, SecretManagerError};
use moka::{future::Cache, policy::EvictionPolicy};
use serde::{Deserialize, Serialize};
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use std::num::ParseIntError;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

///  Config for the database pool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabasePoolCacheConfig {
    /// Database host
    pub host: String,
    /// Database port
    pub port: u16,

    /// Name of the secrets manager secret to use when connecting to
    /// the root "docbox" database
    pub root_secret_name: String,

    /// Max number of active connections per tenant database pool
    ///
    /// This is the maximum number of connections that should be allocated
    /// for performing all queries against each specific tenant.
    ///
    /// Ensure a reasonable amount of connections are allocated but make
    /// sure that the `max_connections` * your number of tenants stays
    /// within the limits for your database
    ///
    /// Default: 10
    pub max_connections: Option<u32>,

    /// Max number of active connections per "docbox" database pool
    ///
    /// This is the maximum number of connections that should be allocated
    /// for performing queries like:
    /// - Listing tenants
    /// - Getting tenant details
    ///
    /// These pools are often short lived and complete their queries very fast
    /// and thus don't need a huge amount of resources allocated to them
    ///
    /// Default: 2
    pub max_connections_root: Option<u32>,

    /// Timeout before a acquiring a database connection is considered
    /// a failure
    ///
    /// Default: 60s
    pub acquire_timeout: Option<u64>,

    /// If a connection has been idle for this duration the connection
    /// will be closed and released back to the database for other
    /// consumers
    ///
    /// Default: 10min
    pub idle_timeout: Option<u64>,

    /// Duration in seconds idle database pools are allowed to be cached before
    /// they are closed
    ///
    /// Default: 48h
    pub cache_duration: Option<u64>,

    /// Maximum database pools to maintain in the cache at once. If the
    /// cache capacity is exceeded old pools will be closed and removed
    /// from the cache
    ///
    /// This capacity should be aligned with your expected number of
    /// tenants along with your `max_connections` to ensure your database
    /// has enough connections to accommodate all tenants.
    ///
    /// Default: 50
    pub cache_capacity: Option<u64>,

    /// Duration in seconds database credentials (host, port, password, ..etc)
    /// are allowed to be cached before they are refresh from the secrets
    /// manager
    ///
    /// Default: 12h
    pub credentials_cache_duration: Option<u64>,

    /// Maximum database credentials to maintain in the cache at once. If the
    /// cache capacity is exceeded old credentials will be removed from the cache
    ///
    /// Default: 50
    pub credentials_cache_capacity: Option<u64>,
}

impl Default for DatabasePoolCacheConfig {
    fn default() -> Self {
        Self {
            host: Default::default(),
            port: 5432,
            root_secret_name: Default::default(),
            max_connections: None,
            max_connections_root: None,
            acquire_timeout: None,
            idle_timeout: None,
            cache_duration: None,
            cache_capacity: None,
            credentials_cache_duration: None,
            credentials_cache_capacity: None,
        }
    }
}

#[derive(Debug, Error)]
pub enum DatabasePoolCacheConfigError {
    #[error("missing DOCBOX_DB_HOST environment variable")]
    MissingDatabaseHost,
    #[error("missing DOCBOX_DB_PORT environment variable")]
    MissingDatabasePort,
    #[error("invalid DOCBOX_DB_PORT environment variable")]
    InvalidDatabasePort,
    #[error("missing DOCBOX_DB_CREDENTIAL_NAME environment variable")]
    MissingDatabaseSecretName,
    #[error("invalid DOCBOX_DB_IDLE_TIMEOUT environment variable")]
    InvalidIdleTimeout(ParseIntError),
    #[error("invalid DOCBOX_DB_ACQUIRE_TIMEOUT environment variable")]
    InvalidAcquireTimeout(ParseIntError),
    #[error("invalid DOCBOX_DB_CACHE_DURATION environment variable")]
    InvalidCacheDuration(ParseIntError),
    #[error("invalid DOCBOX_DB_CACHE_CAPACITY environment variable")]
    InvalidCacheCapacity(ParseIntError),
    #[error("invalid DOCBOX_DB_CREDENTIALS_CACHE_DURATION environment variable")]
    InvalidCredentialsCacheDuration(ParseIntError),
    #[error("invalid DOCBOX_DB_CREDENTIALS_CACHE_CAPACITY environment variable")]
    InvalidCredentialsCacheCapacity(ParseIntError),
}

impl DatabasePoolCacheConfig {
    pub fn from_env() -> Result<DatabasePoolCacheConfig, DatabasePoolCacheConfigError> {
        let db_host: String = std::env::var("DOCBOX_DB_HOST")
            .or(std::env::var("POSTGRES_HOST"))
            .map_err(|_| DatabasePoolCacheConfigError::MissingDatabaseHost)?;
        let db_port: u16 = std::env::var("DOCBOX_DB_PORT")
            .or(std::env::var("POSTGRES_PORT"))
            .map_err(|_| DatabasePoolCacheConfigError::MissingDatabasePort)?
            .parse()
            .map_err(|_| DatabasePoolCacheConfigError::InvalidDatabasePort)?;
        let db_root_secret_name = std::env::var("DOCBOX_DB_CREDENTIAL_NAME")
            .map_err(|_| DatabasePoolCacheConfigError::MissingDatabaseSecretName)?;
        let max_connections: Option<u32> = std::env::var("DOCBOX_DB_MAX_CONNECTIONS")
            .ok()
            .and_then(|value| value.parse().ok());
        let max_connections_root: Option<u32> = std::env::var("DOCBOX_DB_MAX_ROOT_CONNECTIONS")
            .ok()
            .and_then(|value| value.parse().ok());

        let acquire_timeout: Option<u64> = match std::env::var("DOCBOX_DB_ACQUIRE_TIMEOUT") {
            Ok(value) => Some(
                value
                    .parse::<u64>()
                    .map_err(DatabasePoolCacheConfigError::InvalidAcquireTimeout)?,
            ),
            Err(_) => None,
        };

        let idle_timeout: Option<u64> = match std::env::var("DOCBOX_DB_IDLE_TIMEOUT") {
            Ok(value) => Some(
                value
                    .parse::<u64>()
                    .map_err(DatabasePoolCacheConfigError::InvalidIdleTimeout)?,
            ),
            Err(_) => None,
        };

        let cache_duration: Option<u64> = match std::env::var("DOCBOX_DB_CACHE_DURATION") {
            Ok(value) => Some(
                value
                    .parse::<u64>()
                    .map_err(DatabasePoolCacheConfigError::InvalidCacheDuration)?,
            ),
            Err(_) => None,
        };

        let cache_capacity: Option<u64> = match std::env::var("DOCBOX_DB_CACHE_CAPACITY") {
            Ok(value) => Some(
                value
                    .parse::<u64>()
                    .map_err(DatabasePoolCacheConfigError::InvalidCacheCapacity)?,
            ),
            Err(_) => None,
        };

        let credentials_cache_duration: Option<u64> =
            match std::env::var("DOCBOX_DB_CREDENTIALS_CACHE_DURATION") {
                Ok(value) => Some(
                    value
                        .parse::<u64>()
                        .map_err(DatabasePoolCacheConfigError::InvalidCredentialsCacheDuration)?,
                ),
                Err(_) => None,
            };

        let credentials_cache_capacity: Option<u64> =
            match std::env::var("DOCBOX_DB_CREDENTIALS_CACHE_CAPACITY") {
                Ok(value) => Some(
                    value
                        .parse::<u64>()
                        .map_err(DatabasePoolCacheConfigError::InvalidCredentialsCacheCapacity)?,
                ),
                Err(_) => None,
            };

        Ok(DatabasePoolCacheConfig {
            host: db_host,
            port: db_port,
            root_secret_name: db_root_secret_name,
            max_connections,
            max_connections_root,
            acquire_timeout,
            idle_timeout,
            cache_duration,
            cache_capacity,
            credentials_cache_duration,
            credentials_cache_capacity,
        })
    }
}

/// Cache for database pools
pub struct DatabasePoolCache {
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
    secrets_manager: SecretManager,

    /// Max connections per tenant database pool
    max_connections: u32,
    /// Max connections per root database pool
    max_connections_root: u32,

    acquire_timeout: Duration,
    idle_timeout: Duration,
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
    SecretsManager(Box<SecretManagerError>),

    #[error(transparent)]
    Db(#[from] DbErr),

    #[error(transparent)]
    Shared(#[from] Arc<DbConnectErr>),
}

impl DatabasePoolCache {
    pub fn from_config(config: DatabasePoolCacheConfig, secrets_manager: SecretManager) -> Self {
        let cache_duration = Duration::from_secs(config.cache_duration.unwrap_or(60 * 60 * 48));
        let credentials_cache_duration =
            Duration::from_secs(config.credentials_cache_duration.unwrap_or(60 * 60 * 12));

        let cache_capacity = config.cache_capacity.unwrap_or(50);
        let credentials_cache_capacity = config.credentials_cache_capacity.unwrap_or(50);

        let cache = Cache::builder()
            .time_to_idle(cache_duration)
            .max_capacity(cache_capacity)
            .eviction_policy(EvictionPolicy::tiny_lfu())
            .async_eviction_listener(|cache_key: Arc<String>, pool: DbPool, _cause| {
                Box::pin(async move {
                    tracing::debug!(?cache_key, "database pool is no longer in use, closing");
                    pool.close().await
                })
            })
            .build();

        let connect_info_cache = Cache::builder()
            .time_to_idle(credentials_cache_duration)
            .max_capacity(credentials_cache_capacity)
            .eviction_policy(EvictionPolicy::tiny_lfu())
            .build();

        Self {
            host: config.host,
            port: config.port,
            root_secret_name: config.root_secret_name,
            cache,
            connect_info_cache,
            secrets_manager,
            max_connections: config.max_connections.unwrap_or(10),
            max_connections_root: config.max_connections_root.unwrap_or(2),
            idle_timeout: Duration::from_secs(config.idle_timeout.unwrap_or(60 * 10)),
            acquire_timeout: Duration::from_secs(config.acquire_timeout.unwrap_or(60)),
        }
    }

    /// Request a database pool for the root database
    pub async fn get_root_pool(&self) -> Result<PgPool, DbConnectErr> {
        self.get_pool(ROOT_DATABASE_NAME, &self.root_secret_name)
            .await
    }

    /// Request a database pool for a specific tenant
    pub async fn get_tenant_pool(&self, tenant: &Tenant) -> Result<DbPool, DbConnectErr> {
        self.get_pool(&tenant.db_name, &tenant.db_secret_name).await
    }

    /// Closes the database pool for the specific tenant if one is
    /// available and removes the pool from the cache
    pub async fn close_tenant_pool(&self, tenant: &Tenant) {
        let cache_key = format!("{}-{}", &tenant.db_name, &tenant.db_secret_name);
        if let Some(pool) = self.cache.remove(&cache_key).await {
            pool.close().await;
        }

        // Run cache async shutdown jobs
        self.cache.run_pending_tasks().await;
    }

    /// Empties all the caches
    pub async fn flush(&self) {
        // Clear cache
        self.cache.invalidate_all();
        self.connect_info_cache.invalidate_all();
        self.cache.run_pending_tasks().await;
    }

    /// Close all connections in the pool and invalidate the cache
    pub async fn close_all(&self) {
        for (_, value) in self.cache.iter() {
            value.close().await;
        }

        self.flush().await;
    }

    /// Obtains a database pool connection to the database with the provided name
    async fn get_pool(&self, db_name: &str, secret_name: &str) -> Result<DbPool, DbConnectErr> {
        let cache_key = format!("{db_name}-{secret_name}");

        let pool = self
            .cache
            .try_get_with(cache_key, async {
                tracing::debug!(?db_name, "acquiring database pool");

                let pool = self
                    .create_pool(db_name, secret_name)
                    .await
                    .map_err(Arc::new)?;

                Ok(pool)
            })
            .await?;

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
            .parsed_secret::<DbSecrets>(secret_name)
            .await
            .map_err(|err| DbConnectErr::SecretsManager(Box::new(err)))?
            .ok_or(DbConnectErr::MissingCredentials)?;

        // Cache the credential
        self.connect_info_cache
            .insert(secret_name.to_string(), credentials.clone())
            .await;

        Ok(credentials)
    }

    /// Creates a database pool connection
    async fn create_pool(&self, db_name: &str, secret_name: &str) -> Result<DbPool, DbConnectErr> {
        tracing::debug!(?db_name, ?secret_name, "creating db pool connection");

        let credentials = self.get_credentials(secret_name).await?;
        let options = PgConnectOptions::new()
            .host(&self.host)
            .port(self.port)
            .username(&credentials.username)
            .password(&credentials.password)
            .database(db_name);

        let max_connections = match db_name {
            ROOT_DATABASE_NAME => self.max_connections_root,
            _ => self.max_connections,
        };

        match PgPoolOptions::new()
            .max_connections(max_connections)
            // Slightly larger acquire timeout for times when lots of files are being processed
            .acquire_timeout(self.acquire_timeout)
            // Close any connections that have been idle for more than 30min
            .idle_timeout(self.idle_timeout)
            .connect_with(options)
            .await
        {
            // Success case
            Ok(value) => Ok(value),
            Err(err) => {
                // Drop the connect info cache in case the credentials were wrong
                self.connect_info_cache.remove(secret_name).await;
                Err(DbConnectErr::Db(err))
            }
        }
    }
}
