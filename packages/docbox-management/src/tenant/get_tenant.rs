use crate::database::DatabaseProvider;
use docbox_database::{
    DbResult, ROOT_DATABASE_NAME,
    models::tenant::{Tenant, TenantId},
};

pub async fn get_tenant(
    db_provider: &impl DatabaseProvider,
    env: &str,
    tenant_id: TenantId,
) -> DbResult<Option<Tenant>> {
    let db_docbox = db_provider.connect(ROOT_DATABASE_NAME).await?;
    let tenant = Tenant::find_by_id(&db_docbox, tenant_id, env).await?;
    Ok(tenant)
}
