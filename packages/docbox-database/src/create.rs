//! # Create
//!
//! Tenant creation and database setup logic

use crate::{DbPool, DbResult};

/// Creates a new database.
///
/// Running this requires using an account with a higher level of access
/// than the standard db user
pub async fn create_database(db: &DbPool, db_name: &str) -> DbResult<()> {
    let sql = format!(r#"CREATE DATABASE "{db_name}";"#);
    sqlx::raw_sql(&sql).execute(db).await?;

    Ok(())
}

/// Setup the tenants table in the main docbox database
pub async fn create_tenants_table(db: &DbPool) -> DbResult<()> {
    sqlx::raw_sql(
        r#"
-- Setup the tenants table
CREATE TABLE IF NOT EXISTS "docbox_tenants"
(
    "id"              uuid    NOT NULL,
    "db_name"         varchar NOT NULL UNIQUE,
    "db_secret_name"  varchar NOT NULL UNIQUE,
    "s3_name"         varchar NOT NULL UNIQUE,
    "os_index_name"   varchar NOT NULL UNIQUE,
    "env"             varchar NOT NULL,
    "event_queue_url" varchar NULL
)

-- Index tenants across the id and environment (ID alone can be present across multiple tenants)
CREATE UNIQUE INDEX IF NOT EXISTS "idx-tenant-id-env" ON "docbox_tenants" ("id", "env")
    "#,
    )
    .execute(db)
    .await?;

    Ok(())
}

/// Sets up and locks down the tenant user account. This should be
/// run on the tenant database not the root database
///
/// Running this requires using an account with a higher level of access
/// than the standard db user
///
/// `db` - Should be the tenant database
/// `db_name` - Name of the tenant database
/// `role_name` - Name of the user role to create and setup
/// `password` - Password to assign the user role
pub async fn create_tenant_user(
    db: &DbPool,
    db_name: &str,
    role_name: &str,
    password: &str,
) -> DbResult<()> {
    let sql = format!(
        r#"
-- Create database user/role per tenant for docbox api to use
CREATE ROLE {role_name}
LOGIN
PASSWORD '{password}';

-- prevent other pg users with 'public' role from being able to access this database (should have already been done when db was created, but just in case)
REVOKE ALL ON DATABASE "{db_name}" FROM PUBLIC;

--grant all privileges on our schema to our api user;
GRANT ALL ON ALL TABLES IN SCHEMA public TO {role_name};
GRANT ALL ON ALL FUNCTIONS IN SCHEMA public TO {role_name};
GRANT ALL ON ALL SEQUENCES IN SCHEMA public TO {role_name};

GRANT CREATE ON SCHEMA public TO {role_name};


-- ensure our api user can access any new objects created in future
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON TABLES TO {role_name};
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON FUNCTIONS TO {role_name};
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON SEQUENCES TO {role_name};

-- ensure our api user can connect to the db
GRANT CONNECT ON DATABASE "{db_name}" TO {role_name};    
    "#
    );

    sqlx::raw_sql(&sql).execute(db).await?;

    Ok(())
}
