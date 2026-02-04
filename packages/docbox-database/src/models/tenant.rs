use crate::{DbExecutor, DbResult};
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use uuid::Uuid;

pub type TenantId = Uuid;

use crate::utils::update_if_some;

#[derive(Debug, Clone, FromRow, Serialize, PartialEq, Eq)]
pub struct Tenant {
    /// Unique ID for the tenant
    pub id: TenantId,
    /// Name for the tenant
    pub name: String,
    /// Name of the tenant database
    pub db_name: String,
    /// Name for the AWS secret used for the database user if
    /// using secret based authentication
    pub db_secret_name: Option<String>,
    /// Name for the database user username if using IAM based
    /// authentication
    #[sqlx(default)]
    pub db_iam_user_name: Option<String>,
    /// Name of the tenant s3 bucket
    pub s3_name: String,
    /// Name of the tenant search index
    pub os_index_name: String,
    /// Environment for the tenant
    pub env: String,
    /// Optional event queue (SQS) to send docbox events to
    pub event_queue_url: Option<String>,
}

/// Structure for fields required when creating a
/// tenant within the database
pub struct CreateTenant {
    pub id: TenantId,
    pub name: String,
    pub db_name: String,
    pub db_iam_user_name: Option<String>,
    pub db_secret_name: Option<String>,
    pub s3_name: String,
    pub os_index_name: String,
    pub event_queue_url: Option<String>,
    pub env: String,
}

/// Bulk update for tenant fields
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateTenant {
    pub id: Option<TenantId>,
    pub name: Option<String>,
    pub db_name: Option<String>,
    pub db_secret_name: Option<Option<String>>,
    pub db_iam_user_name: Option<Option<String>>,
    pub s3_name: Option<String>,
    pub os_index_name: Option<String>,
    pub env: Option<String>,
    pub event_queue_url: Option<Option<String>>,
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
                "db_iam_user_name",
                "db_secret_name",
                "s3_name",
                "os_index_name",
                "env",
                "event_queue_url"
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        "#,
        )
        .bind(create.id)
        .bind(create.name.as_str())
        .bind(create.db_name.as_str())
        .bind(create.db_iam_user_name.clone())
        .bind(create.db_secret_name.clone())
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
            db_iam_user_name: create.db_iam_user_name,
            db_secret_name: create.db_secret_name,
            s3_name: create.s3_name,
            os_index_name: create.os_index_name,
            env: create.env,
            event_queue_url: create.event_queue_url,
        })
    }

    /// Update the "db_iam_user_name" property of the tenant
    pub async fn update(
        &mut self,
        db: impl DbExecutor<'_>,
        UpdateTenant {
            id,
            name,
            db_name,
            db_secret_name,
            db_iam_user_name,
            s3_name,
            os_index_name,
            env,
            event_queue_url,
        }: UpdateTenant,
    ) -> DbResult<()> {
        sqlx::query(
            r#"
            UPDATE "docbox_tenants"
            SET
                "id" = COALESCE($3, "id"),
                "name" = COALESCE($4, "name"),
                "db_name" = COALESCE($5, "db_name"),
                "db_secret_name" = COALESCE($6, "db_secret_name"),
                "db_iam_user_name" = COALESCE($7, "db_iam_user_name"),
                "s3_name" = COALESCE($8, "s3_name"),
                "os_index_name" = COALESCE($9, "os_index_name"),
                "env" = COALESCE($10, "env"),
                "event_queue_url" = COALESCE($11, "event_queue_url")
            WHERE "id" = $1 AND "env" = $2
            "#,
        )
        //
        .bind(self.id)
        .bind(self.env.clone())
        //
        .bind(id)
        .bind(name.clone())
        .bind(db_name.clone())
        .bind(db_secret_name.clone())
        .bind(db_iam_user_name.clone())
        .bind(s3_name.clone())
        .bind(os_index_name.clone())
        .bind(env.clone())
        .bind(event_queue_url.clone())
        .fetch_optional(db)
        .await?;

        update_if_some!(
            self,
            id,
            name,
            db_name,
            db_secret_name,
            db_iam_user_name,
            s3_name,
            os_index_name,
            env,
            event_queue_url,
        );

        Ok(())
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
        sqlx::query_as(r#"SELECT * FROM "docbox_tenants" WHERE "env" = $1 ORDER BY "name""#)
            .bind(env)
            .fetch_all(db)
            .await
    }

    /// Finds all tenants
    pub async fn all(db: impl DbExecutor<'_>) -> DbResult<Vec<Tenant>> {
        sqlx::query_as(r#"SELECT * FROM "docbox_tenants" ORDER BY "name""#)
            .fetch_all(db)
            .await
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
