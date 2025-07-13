use docbox_database::{
    DbErr, ROOT_DATABASE_NAME,
    models::tenant::{Tenant, TenantId},
};
use thiserror::Error;

use crate::database::DatabaseProvider;

#[derive(Debug, Error)]
pub enum DeleteTenantError {
    #[error(transparent)]
    Database(DbErr),

    #[error("tenant not found")]
    TenantNotFound,

    #[error("failed to delete tenant: {0}")]
    DeleteTenant(DbErr),
}

pub async fn delete_tenant(
    db_provider: &impl DatabaseProvider,
    env: &str,
    tenant_id: TenantId,
) -> Result<(), DeleteTenantError> {
    let db_docbox = db_provider
        .connect(ROOT_DATABASE_NAME)
        .await
        .map_err(DeleteTenantError::Database)?;
    let tenant = Tenant::find_by_id(&db_docbox, tenant_id, env)
        .await
        .map_err(DeleteTenantError::Database)?
        .ok_or(DeleteTenantError::TenantNotFound)?;

    // ..TODO: Optionally delete S3 bucket, opensearch index, database

    tenant
        .delete(&db_docbox)
        .await
        .map_err(DeleteTenantError::DeleteTenant)?;

    Ok(())
}
