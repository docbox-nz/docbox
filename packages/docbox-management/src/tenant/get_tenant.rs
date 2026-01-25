use crate::database::{DatabaseProvider, close_pool_on_drop};
use docbox_core::database::{
    DbResult, ROOT_DATABASE_NAME,
    models::tenant::{Tenant, TenantId},
};

#[tracing::instrument(skip(db_provider))]
pub async fn get_tenant(
    db_provider: &impl DatabaseProvider,
    env: &str,
    tenant_id: TenantId,
) -> DbResult<Option<Tenant>> {
    let db_docbox = db_provider.connect(ROOT_DATABASE_NAME).await?;
    let _guard = close_pool_on_drop(&db_docbox);

    let tenant = Tenant::find_by_id(&db_docbox, tenant_id, env).await?;
    Ok(tenant)
}
