use crate::database::DatabaseProvider;
use docbox_database::{DbResult, ROOT_DATABASE_NAME, models::tenant::Tenant};

pub async fn get_tenants(db_provider: &impl DatabaseProvider) -> DbResult<Vec<Tenant>> {
    let db_docbox = db_provider.connect(ROOT_DATABASE_NAME).await?;
    let tenants = Tenant::all(&db_docbox).await?;
    Ok(tenants)
}
