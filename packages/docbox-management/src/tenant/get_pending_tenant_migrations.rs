use crate::database::DatabaseProvider;
use docbox_database::{DbResult, ROOT_DATABASE_NAME, models::tenant::Tenant};

#[tracing::instrument(skip(db_provider))]
pub async fn get_pending_tenant_migrations(
    db_provider: &impl DatabaseProvider,
    tenant: &Tenant,
) -> DbResult<Vec<String>> {
    // Connect to the root database
    let root_db = db_provider.connect(ROOT_DATABASE_NAME).await?;
    let migrations =
        docbox_database::migrations::get_pending_tenant_migrations(&root_db, tenant).await?;
    Ok(migrations)
}
