use super::{DbPool, DbResult, DbTransaction};
use std::ops::DerefMut;

const MIGRATIONS: &[(&str, &str)] = &[
    (
        "m1_create_users_table",
        include_str!("./migrations/m1_create_users_table.sql"),
    ),
    (
        "m2_create_document_box_table",
        include_str!("./migrations/m2_create_document_box_table.sql"),
    ),
    (
        "m3_create_folders_table",
        include_str!("./migrations/m3_create_folders_table.sql"),
    ),
    (
        "m4_create_files_table",
        include_str!("./migrations/m4_create_files_table.sql"),
    ),
    (
        "m5_create_generated_files_table",
        include_str!("./migrations/m5_create_generated_files_table.sql"),
    ),
    (
        "m6_create_links_table",
        include_str!("./migrations/m6_create_links_table.sql"),
    ),
    (
        "m7_create_edit_history_table",
        include_str!("./migrations/m7_create_edit_history_table.sql"),
    ),
    (
        "m8_create_tasks_table",
        include_str!("./migrations/m8_create_tasks_table.sql"),
    ),
    (
        "m9_create_presigned_upload_tasks_table",
        include_str!("./migrations/m9_create_presigned_upload_tasks_table.sql"),
    ),
];

/// Creates all the tables for a specific tenant
pub async fn create_tenant_tables(db: &mut DbTransaction<'_>) -> DbResult<()> {
    for (name, sql) in MIGRATIONS {
        if let Err(cause) = sqlx::query(sql).execute(db.deref_mut()).await {
            tracing::error!(?cause, ?name, "failed to perform migration");
            return Err(cause);
        }
    }

    Ok(())
}

/// Apply a specific migration by name
pub async fn apply_migration(db: &DbPool, migration_name: &str) -> DbResult<()> {
    let mut t = db.begin().await?;

    for (name, sql) in MIGRATIONS {
        if (*name).eq(migration_name) {
            if let Err(cause) = sqlx::query(sql).execute(t.deref_mut()).await {
                tracing::error!(?cause, ?name, "failed to perform migration");
                return Err(cause);
            }
        }
    }

    t.commit().await?;

    Ok(())
}
