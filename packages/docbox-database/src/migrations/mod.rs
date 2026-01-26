use crate::{
    DbExecutor, DbResult, DbTransaction,
    models::{
        root_migration::{CreateRootMigration, RootMigration},
        tenant::Tenant,
        tenant_migration::{CreateTenantMigration, TenantMigration},
    },
};
use chrono::Utc;
use std::ops::DerefMut;

pub const ROOT_MIGRATIONS: &[(&str, &str)] = &[
    (
        "m1_create_tenants_table",
        include_str!("./root/m1_create_tenants_table.sql"),
    ),
    (
        "m2_create_tenant_migrations_table",
        include_str!("./root/m2_create_tenant_migrations_table.sql"),
    ),
    (
        "m3_create_storage_bucket_index",
        include_str!("./root/m3_create_storage_bucket_index.sql"),
    ),
    (
        "m4_tenant_migrations_update_constraint",
        include_str!("./root/m4_tenant_migrations_update_constraint.sql"),
    ),
    (
        "m5_tenant_iam_support",
        include_str!("./root/m5_tenant_iam_support.sql"),
    ),
];

pub const TENANT_MIGRATIONS: &[(&str, &str)] = &[
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
    (
        "m10_add_pinned_column",
        include_str!("./tenant/m10_add_pinned_column.sql"),
    ),
    (
        "m11_create_links_resolved_metadata_table",
        include_str!("./tenant/m11_create_links_resolved_metadata_table.sql"),
    ),
    (
        "m12_create_composite_types_and_views",
        include_str!("./tenant/m12_create_composite_types_and_views.sql"),
    ),
    (
        "m13_create_folder_functions",
        include_str!("./tenant/m13_create_folder_functions.sql"),
    ),
    (
        "m14_create_link_functions",
        include_str!("./tenant/m14_create_link_functions.sql"),
    ),
    (
        "m15_create_file_functions",
        include_str!("./tenant/m15_create_file_functions.sql"),
    ),
    (
        "m16_docbox_tasks_constraint",
        include_str!("./tenant/m16_docbox_tasks_constraint.sql"),
    ),
];

/// Initialize the table used for root migration tracking
///
/// (Must be performed before normal migrations can happen otherwise tracking will fail)
pub async fn initialize_root_migrations(db: impl DbExecutor<'_>) -> DbResult<()> {
    sqlx::raw_sql(include_str!("./root/m0_create_root_migrations_table.sql"))
        .execute(db)
        .await?;

    Ok(())
}

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

/// Get all pending migrations for the root that have not been applied yet
pub async fn get_pending_root_migrations(db: impl DbExecutor<'_>) -> DbResult<Vec<String>> {
    let migrations = RootMigration::all(db).await?;

    let pending = ROOT_MIGRATIONS
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
        apply_migration(t, migration_name, migration).await?;

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

/// Applies migrations to the root, only applies migrations that
/// haven't already been applied
///
/// Optionally filtered to a specific migration through `target_migration_name`
pub async fn apply_root_migrations(
    root_t: &mut DbTransaction<'_>,
    target_migration_name: Option<&str>,
) -> DbResult<()> {
    let migrations = RootMigration::all(root_t.deref_mut()).await?;

    for (migration_name, migration) in ROOT_MIGRATIONS {
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
        apply_migration(root_t, migration_name, migration).await?;

        // Store the applied migration
        RootMigration::create(
            root_t.deref_mut(),
            CreateRootMigration {
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

        apply_migration(t, migration_name, migration).await?;
    }

    Ok(())
}

/// Apply a migration to the specific database
pub async fn apply_migration(
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
