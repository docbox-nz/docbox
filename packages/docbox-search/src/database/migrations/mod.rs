use crate::{DatabaseSearchError, SearchError};

/// Collection of migrations to apply against the database for the database
/// backed search index
const TENANT_MIGRATIONS: &[(&str, &str)] = &[
    (
        "m1_create_additional_indexes",
        include_str!("./m1_create_additional_indexes.sql"),
    ),
    (
        "m2_search_create_files_pages_table",
        include_str!("./m2_search_create_files_pages_table.sql"),
    ),
    (
        "m3_create_tsvector_columns",
        include_str!("./m3_create_tsvector_columns.sql"),
    ),
    (
        "m4_search_functions_and_types",
        include_str!("./m4_search_functions_and_types.sql"),
    ),
];

pub fn get_pending_migrations(applied_names: Vec<String>) -> Vec<String> {
    TENANT_MIGRATIONS
        .iter()
        .filter(|(migration_name, _migration)| {
            // Skip already applied migrations
            !applied_names
                .iter()
                .any(|applied_migration| applied_migration.eq(migration_name))
        })
        .map(|(migration_name, _migration)| migration_name.to_string())
        .collect()
}

pub async fn apply_migration(
    t: &mut docbox_database::DbTransaction<'_>,
    name: &str,
) -> Result<(), SearchError> {
    let (_, migration) = TENANT_MIGRATIONS
        .iter()
        .find(|(migration_name, _)| name.eq(*migration_name))
        .ok_or(DatabaseSearchError::MigrationNotFound)?;

    // Apply the migration
    docbox_database::migrations::apply_migration(t, name, migration)
        .await
        .inspect_err(|error| tracing::error!(?error, "failed to apply migration"))
        .map_err(DatabaseSearchError::ApplyMigration)?;

    Ok(())
}
