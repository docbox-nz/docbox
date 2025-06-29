use crate::{DbExecutor, DbResult, models::tenant::TenantId};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::prelude::FromRow;

/// Structure for tracking migrations applied to a tenant
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct TenantMigration {
    pub tenant_id: TenantId,
    pub env: String,
    pub name: String,
    pub applied_at: DateTime<Utc>,
}

pub struct CreateTenantMigration {
    pub tenant_id: TenantId,
    pub env: String,
    pub name: String,
    pub applied_at: DateTime<Utc>,
}

impl TenantMigration {
    /// Create a new tenant migration
    pub async fn create(db: impl DbExecutor<'_>, create: CreateTenantMigration) -> DbResult<()> {
        sqlx::query(
            r#"
            INSERT INTO "docbox_tenants_migrations" (
                "env",
                "tenant_id",
                "name",
                "applied_at"
            )
            VALUES ($1, $2, $3, $4)
        "#,
        )
        .bind(create.env)
        .bind(create.tenant_id)
        .bind(create.name)
        .bind(create.applied_at)
        .execute(db)
        .await?;

        Ok(())
    }

    /// Find all migrations for a tenant by `tenant_id` within a specific `env`
    pub async fn find_by_tenant(
        db: impl DbExecutor<'_>,
        tenant_id: TenantId,
        env: &str,
    ) -> DbResult<Vec<TenantMigration>> {
        sqlx::query_as(
            r#"SELECT * FROM "docbox_tenants_migrations" WHERE "env" = $1 AND "tenant_id" = $2"#,
        )
        .bind(env)
        .bind(tenant_id)
        .fetch_all(db)
        .await
    }
}
