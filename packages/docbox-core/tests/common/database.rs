use docbox_database::{
    migrations::force_apply_tenant_migrations, DbPool, PgConnectOptions, PgPoolOptions,
};
use testcontainers_modules::{postgres::Postgres, testcontainers::ContainerAsync};

/// Testing utility to create and setup a database for a tenant to use in tests that
/// require database access
///
/// Requires that the test runner have docker available to launch the postgres
/// container that will be used
pub async fn create_test_tenant_database() -> (ContainerAsync<Postgres>, DbPool) {
    use testcontainers_modules::testcontainers::runners::AsyncRunner;

    let container = testcontainers_modules::postgres::Postgres::default()
        .with_db_name("docbox_test")
        .with_user("docbox")
        .with_password("docbox")
        .start()
        .await
        .unwrap();
    let host_ip = container.get_host().await.unwrap();
    let host_port = container.get_host_port_ipv4(5432).await.unwrap();

    let options = PgConnectOptions::new()
        .host(&host_ip.to_string())
        .port(host_port)
        .username("docbox")
        .password("docbox")
        .database("docbox_test")
        .ssl_mode(docbox_database::PgSslMode::Disable);

    let db = PgPoolOptions::new().connect_with(options).await.unwrap();
    let mut trans = db.begin().await.unwrap();
    force_apply_tenant_migrations(&mut trans, None)
        .await
        .unwrap();
    trans.commit().await.unwrap();
    (container, db)
}
