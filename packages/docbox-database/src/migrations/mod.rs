use crate::{
    DbExecutor, DbResult, DbTransaction,
    models::{
        tenant::Tenant,
        tenant_migration::{CreateTenantMigration, TenantMigration},
    },
};
use chrono::Utc;
use std::ops::DerefMut;

const TENANT_MIGRATIONS: &[(&str, &str)] = &[
    (
        "m1_create_users_table",
        include_str!("./tenant/m1_create_users_table.sql"),
    ),
    (
        "m2_create_document_box_table",
        include_str!("./tenant/m2_create_document_box_table.sql"),
    ),
    (
        "m3_create_folders_table",
        include_str!("./tenant/m3_create_folders_table.sql"),
    ),
    (
        "m4_create_files_table",
        include_str!("./tenant/m4_create_files_table.sql"),
    ),
    (
        "m5_create_generated_files_table",
        include_str!("./tenant/m5_create_generated_files_table.sql"),
    ),
    (
        "m6_create_links_table",
        include_str!("./tenant/m6_create_links_table.sql"),
    ),
    (
        "m7_create_edit_history_table",
        include_str!("./tenant/m7_create_edit_history_table.sql"),
    ),
    (
        "m8_create_tasks_table",
        include_str!("./tenant/m8_create_tasks_table.sql"),
    ),
    (
        "m9_create_presigned_upload_tasks_table",
        include_str!("./tenant/m9_create_presigned_upload_tasks_table.sql"),
    ),
];

/// Get all pending migrations for a tenant that have not been applied yet
pub async fn get_pending_tenant_migrations(
    db: impl DbExecutor<'_>,
    tenant: &Tenant,
) -> DbResult<Vec<String>> {
    let migrations = TenantMigration::find_by_tenant(db, tenant.id, &tenant.env).await?;

    let pending = TENANT_MIGRATIONS
        .iter()
        .filter(|(migration_name, _migration)| {
            // Skip already applied migrations
            !migrations
                .iter()
                .any(|migration| migration.name.eq(migration_name))
        })
        .map(|(migration_name, _migration)| migration_name.to_string())
        .collect();

    Ok(pending)
}

/// Applies migrations to the provided tenant, only applies migrations that
/// haven't already been applied
///
/// Optionally filtered to a specific migration through `target_migration_name`
pub async fn apply_tenant_migrations(
    root_t: &mut DbTransaction<'_>,
    t: &mut DbTransaction<'_>,
    tenant: &Tenant,
    target_migration_name: Option<&str>,
) -> DbResult<()> {
    let migrations =
        TenantMigration::find_by_tenant(root_t.deref_mut(), tenant.id, &tenant.env).await?;

    for (migration_name, migration) in TENANT_MIGRATIONS {
        // If targeting a specific migration only apply the target one
        if target_migration_name
            .is_some_and(|target_migration_name| target_migration_name.ne(*migration_name))
        {
            continue;
        }

        // Skip already applied migrations
        if migrations
            .iter()
            .any(|migration| migration.name.eq(migration_name))
        {
            continue;
        }

        // Apply the migration
        apply_tenant_migration(t, migration_name, migration).await?;

        // Store the applied migration
        TenantMigration::create(
            root_t.deref_mut(),
            CreateTenantMigration {
                tenant_id: tenant.id,
                env: tenant.env.clone(),
                name: migration_name.to_string(),
                applied_at: Utc::now(),
            },
        )
        .await?;
    }

    Ok(())
}

/// Applies migrations without checking if migrations have already been applied
///
/// Should only be used for integration tests where you aren't setting up the root database
pub async fn force_apply_tenant_migrations(
    t: &mut DbTransaction<'_>,
    target_migration_name: Option<&str>,
) -> DbResult<()> {
    for (migration_name, migration) in TENANT_MIGRATIONS {
        // If targeting a specific migration only apply the target one
        if target_migration_name
            .is_some_and(|target_migration_name| target_migration_name.ne(*migration_name))
        {
            continue;
        }

        apply_tenant_migration(t, migration_name, migration).await?;
    }

    Ok(())
}

/// Apply a migration to the specific tenant database
pub async fn apply_tenant_migration(
    db: &mut DbTransaction<'_>,
    migration_name: &str,
    migration: &str,
) -> DbResult<()> {
    // Split the SQL queries into multiple queries
    let queries = migration
        .split(';')
        .map(|query| query.trim())
        .filter(|query| !query.is_empty());

    for query in queries {
        let result = sqlx::query(query)
            .execute(db.deref_mut())
            .await
            .inspect_err(|error| {
                tracing::error!(?error, ?migration_name, "failed to perform migration")
            })?;
        let rows_affected = result.rows_affected();

        tracing::debug!(?migration_name, ?rows_affected, "applied migration query");
    }

    Ok(())
}
