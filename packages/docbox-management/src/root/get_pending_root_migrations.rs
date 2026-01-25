use crate::database::{DatabaseProvider, close_pool_on_drop};
use docbox_core::database::{
    DbResult, ROOT_DATABASE_NAME, create::check_database_table_exists, migrations::ROOT_MIGRATIONS,
};

/// Get migrations that are pending on the root database
#[tracing::instrument(skip(db_provider))]
pub async fn get_pending_root_migrations(
    db_provider: &impl DatabaseProvider,
) -> DbResult<Vec<String>> {
    let root_db = db_provider.connect(ROOT_DATABASE_NAME).await?;
    let _guard = close_pool_on_drop(&root_db);

    // Check if the migrations table has been initialized, this did not exist before v0.4.0
    // in this case return all the migrations as they will all need to run before this as the
    // "get_pending_root_migrations" code will fail without the database being created
    if !check_database_table_exists(&root_db, "docbox_root_migrations").await? {
        return Ok(ROOT_MIGRATIONS
            .iter()
            .map(|(migration_name, _migration)| migration_name.to_string())
            .collect());
    }

    let migrations = docbox_core::database::migrations::get_pending_root_migrations(&root_db).await?;
    Ok(migrations)
}
