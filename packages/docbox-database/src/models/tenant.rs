use crate::{DbExecutor, DbResult};
use serde::Serialize;
use sqlx::prelude::FromRow;
use uuid::Uuid;

pub type TenantId = Uuid;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Tenant {
    /// Unique ID for the tenant
    pub id: TenantId,
    /// Name for the tenant
    pub name: String,
    /// Name of the tenant database
    pub db_name: String,
    /// Name for the AWS secret used for the database user
    pub db_secret_name: String,
    /// Name of the tenant s3 bucket
    pub s3_name: String,
    /// Name of the tenant search index
    pub os_index_name: String,
    /// Environment for the tenant
    pub env: String,
    /// Optional event queue (SQS) to send docbox events to
    pub event_queue_url: Option<String>,
}

pub struct CreateTenant {
    pub id: TenantId,
    pub name: String,
    pub db_name: String,
    pub db_secret_name: String,
    pub s3_name: String,
    pub os_index_name: String,
    pub event_queue_url: Option<String>,
    pub env: String,
}

impl Tenant {
    /// Create a new tenant
    pub async fn create(db: impl DbExecutor<'_>, create: CreateTenant) -> DbResult<Tenant> {
        sqlx::query(
            r#"
            INSERT INTO "docbox_tenants" (
                "id",
                "name",
                "db_name",
                "db_secret_name",
                "s3_name",
                "os_index_name",
                "env",
                "event_queue_url"
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
        )
        .bind(create.id)
        .bind(create.name.as_str())
        .bind(create.db_name.as_str())
        .bind(create.db_secret_name.as_str())
        .bind(create.s3_name.as_str())
        .bind(create.os_index_name.as_str())
        .bind(create.env.as_str())
        .bind(create.event_queue_url.as_ref())
        .execute(db)
        .await?;

        Ok(Tenant {
            id: create.id,
            name: create.name,
            db_name: create.db_name,
            db_secret_name: create.db_secret_name,
            s3_name: create.s3_name,
            os_index_name: create.os_index_name,
            env: create.env,
            event_queue_url: create.event_queue_url,
        })
    }

    /// Find a tenant by `id` within a specific `env`
    pub async fn find_by_id(
        db: impl DbExecutor<'_>,
        id: TenantId,
        env: &str,
    ) -> DbResult<Option<Tenant>> {
        sqlx::query_as(r#"SELECT * FROM "docbox_tenants" WHERE "id" = $1 AND "env" = $2"#)
            .bind(id)
            .bind(env)
            .fetch_optional(db)
            .await
    }

    /// Find a tenant using its S3 bucket
    pub async fn find_by_bucket(db: impl DbExecutor<'_>, bucket: &str) -> DbResult<Option<Tenant>> {
        sqlx::query_as(r#"SELECT * FROM "docbox_tenants" WHERE "s3_name" = $1"#)
            .bind(bucket)
            .fetch_optional(db)
            .await
    }

    /// Finds all tenants for the specified environment
    pub async fn find_by_env(db: impl DbExecutor<'_>, env: &str) -> DbResult<Vec<Tenant>> {
        sqlx::query_as(r#"SELECT * FROM "docbox_tenants" WHERE "env" = $1"#)
            .bind(env)
            .fetch_all(db)
            .await
    }

    /// Finds all tenants
    pub async fn all(db: impl DbExecutor<'_>) -> DbResult<Vec<Tenant>> {
        sqlx::query_as(r#"SELECT * FROM "docbox_tenants""#)
            .fetch_all(db)
            .await
    }

    /// Update the open search index for the tenant
    pub async fn update_os_index(
        &mut self,
        db: impl DbExecutor<'_>,
        os_index_name: String,
    ) -> DbResult<()> {
        sqlx::query(
            r#"UPDATE "docbox_tenants" SET "os_index_name" = $1 WHERE "id" = $2 AND "env" = $3"#,
        )
        .bind(&os_index_name)
        .bind(self.id)
        .bind(&self.env)
        .execute(db)
        .await?;

        self.os_index_name = os_index_name;
        Ok(())
    }

    /// Deletes the tenant
    pub async fn delete(self, db: impl DbExecutor<'_>) -> DbResult<()> {
        sqlx::query(r#"DELETE FROM "docbox_tenants" WHERE "id" = $1 AND "env" = $2"#)
            .bind(self.id)
            .bind(&self.env)
            .execute(db)
            .await?;
        Ok(())
    }
}
