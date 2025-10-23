use crate::database::{DatabaseProvider, close_pool_on_drop};
use docbox_database::{DbResult, ROOT_DATABASE_NAME, models::tenant::Tenant};

#[tracing::instrument(skip_all)]
pub async fn get_tenants(db_provider: &impl DatabaseProvider) -> DbResult<Vec<Tenant>> {
    let db_docbox = db_provider.connect(ROOT_DATABASE_NAME).await?;
    let _guard = close_pool_on_drop(&db_docbox);

    let tenants = Tenant::all(&db_docbox).await?;
    Ok(tenants)
}
