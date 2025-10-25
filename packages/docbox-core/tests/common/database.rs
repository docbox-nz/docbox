use docbox_database::{
    DbPool, PgConnectOptions, PgPoolOptions, migrations::force_apply_tenant_migrations,
};
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::{postgres::Postgres, testcontainers::ContainerAsync};

const TEST_DB_NAME: &str = "docbox_test";
const TEST_DB_USER: &str = "docbox";
const TEST_DB_PASSWORD: &str = "docbox";

pub async fn test_database_container() -> ContainerAsync<Postgres> {
    testcontainers_modules::postgres::Postgres::default()
        .with_db_name(TEST_DB_NAME)
        .with_user(TEST_DB_USER)
        .with_password(TEST_DB_PASSWORD)
        .start()
        .await
        .unwrap()
}

/// Testing utility to create and setup a database for a tenant to use in tests that
/// require database access
///
/// Requires that the test runner have docker available to launch the postgres
/// container that will be used
///
/// Marked with #[allow(dead_code)] as it is used by tests but
/// rustc doesn't believe us
#[allow(dead_code)]
pub async fn test_tenant_database(container: &ContainerAsync<Postgres>) -> DbPool {
    let host_ip = container.get_host().await.unwrap();
    let host_port = container.get_host_port_ipv4(5432).await.unwrap();

    let options = PgConnectOptions::new()
        .host(&host_ip.to_string())
        .port(host_port)
        .username(TEST_DB_USER)
        .password(TEST_DB_PASSWORD)
        .database(TEST_DB_NAME)
        .ssl_mode(docbox_database::sqlx::postgres::PgSslMode::Disable);

    let db = PgPoolOptions::new().connect_with(options).await.unwrap();
    let mut trans = db.begin().await.unwrap();
    force_apply_tenant_migrations(&mut trans, None)
        .await
        .unwrap();
    trans.commit().await.unwrap();
    db
}

#[allow(dead_code)]
pub async fn test_tenant_db() -> (DbPool, ContainerAsync<Postgres>) {
    let db_container = test_database_container().await;
    let db = test_tenant_database(&db_container).await;

    (db, db_container)
}
