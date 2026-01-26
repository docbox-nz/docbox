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

/// Check if a database with the provided `db_name` exists
pub async fn check_database_exists(db: &DbPool, db_name: &str) -> DbResult<bool> {
    let result = sqlx::query("SELECT 1 FROM pg_database WHERE datname = $1")
        .bind(db_name)
        .fetch_optional(db)
        .await?;

    Ok(result.is_some())
}

/// Check if a table with the provided `table_name` exists
pub async fn check_database_table_exists(db: &DbPool, table_name: &str) -> DbResult<bool> {
    let result = sqlx::query(
        "SELECT 1 FROM pg_catalog.pg_tables
        WHERE schemaname = 'public'
          AND tablename  = $1",
    )
    .bind(table_name)
    .fetch_optional(db)
    .await?;

    Ok(result.is_some())
}

/// Check if a database role with the provided `role_name` exists
pub async fn check_database_role_exists(db: &DbPool, role_name: &str) -> DbResult<bool> {
    let result = sqlx::query("SELECT 1 FROM pg_roles WHERE rolname = $1")
        .bind(role_name)
        .fetch_optional(db)
        .await?;

    Ok(result.is_some())
}

/// Delete a database.
///
/// Running this requires using an account with a higher level of access
/// than the standard db user
pub async fn delete_database(db: &DbPool, db_name: &str) -> DbResult<()> {
    let sql = format!(r#"DROP DATABASE "{db_name}";"#);
    sqlx::raw_sql(&sql).execute(db).await?;

    Ok(())
}

/// Sets up and locks down a database role.
///
/// Running this requires using an account with a higher level of access
/// than the standard db user
///
/// `db` - Should be the tenant database
/// `db_name` - Name of the tenant database
/// `role_name` - Name of the user role to create and setup
/// `password` - Password to assign the user role
pub async fn create_restricted_role(
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

/// Sets up and locks down a database role.
///
/// This database role is granted rds_iam to allow access to this
/// role through AWS IAM
///
/// Running this requires using an account with a higher level of access
/// than the standard db user.
///
/// `db` - Should be the tenant database
/// `db_name` - Name of the tenant database
/// `role_name` - Name of the user role to create and setup
pub async fn create_restricted_role_aws_iam(
    db: &DbPool,
    db_name: &str,
    role_name: &str,
) -> DbResult<()> {
    let sql = format!(
        r#"
-- Create database user/role per tenant for docbox api to use
CREATE ROLE {role_name}
LOGIN;

-- Allow IAM authentication
GRANT rds_iam TO {role_name};

-- prevent other pg users with 'public' role from being able to access this database (should have already been done when db was created, but just in case)
REVOKE ALL ON DATABASE "{db_name}" FROM PUBLIC;

--grant all privileges on our schema to our api user;
GRANT ALL ON ALL TABLES IN SCHEMA public TO {role_name};
GRANT ALL ON ALL FUNCTIONS IN SCHEMA public TO {role_name};
GRANT ALL ON ALL SEQUENCES IN SCHEMA public TO {role_name};

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

/// Delete a database role.
///
/// Running this requires using an account with a higher level of access
/// than the standard db user
pub async fn delete_role(db: &DbPool, role_name: &str) -> DbResult<()> {
    let sql = format!(r#"DROP ROLE IF EXISTS "{role_name}";"#);
    sqlx::raw_sql(&sql).execute(db).await?;

    Ok(())
}
