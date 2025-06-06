//! # Tenant Cache
//!
//! Provides caching for tenants to ensure we don't have to fetch the tenant
//! from the database for every request

use docbox_database::{
    DbPool, DbResult,
    models::tenant::{Tenant, TenantId},
};
use moka::{future::Cache, policy::EvictionPolicy};
use std::time::Duration;

/// Duration to maintain tenant caches (15 minutes)
const TENANT_CACHE_DURATION: Duration = Duration::from_secs(60 * 15);

/// Maximum tenants to keep in cache
const TENANT_CACHE_CAPACITY: u64 = 50;

/// Cache for recently used tenants
#[derive(Clone)]
pub struct TenantCache {
    cache: Cache<TenantCacheKey, Tenant>,
}

/// Cache key to identify a tenant
#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
struct TenantCacheKey {
    env: String,
    tenant_id: TenantId,
}

impl Default for TenantCache {
    fn default() -> Self {
        Self::new()
    }
}

impl TenantCache {
    /// Create a new tenant cache
    pub fn new() -> Self {
        let cache = Cache::builder()
            .time_to_idle(TENANT_CACHE_DURATION)
            .max_capacity(TENANT_CACHE_CAPACITY)
            .eviction_policy(EvictionPolicy::tiny_lfu())
            .build();

        Self { cache }
    }

    /// Get a tenant by ID
    pub async fn get_tenant(
        &self,
        db: &DbPool,
        env: String,
        tenant_id: TenantId,
    ) -> DbResult<Option<Tenant>> {
        let cache_key = TenantCacheKey { env, tenant_id };

        if let Some(tenant) = self.cache.get(&cache_key).await {
            return Ok(Some(tenant.clone()));
        }

        let tenant = Tenant::find_by_id(db, tenant_id, &cache_key.env).await?;

        if let Some(tenant) = tenant.as_ref() {
            self.cache.insert(cache_key, tenant.clone()).await;
        }

        Ok(tenant)
    }

    /// Clear the cache
    pub async fn flush(&self) {
        self.cache.invalidate_all();
    }
}
