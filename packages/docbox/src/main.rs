use anyhow::Context;
use axum::{Extension, extract::DefaultBodyLimit, routing::post};
use docbox_core::{
    aws::{SqsClient, aws_config},
    background::{BackgroundTaskData, perform_background_tasks},
    events::{EventPublisherFactory, sqs::SqsEventPublisherFactory},
    notifications::{
        AppNotificationQueue, NotificationConfig,
        process::{NotificationQueueData, process_notification_queue},
    },
    processing::{
        ProcessingLayer,
        office::{OfficeConverter, OfficeProcessingLayer, convert_server::OfficeConverterServer},
    },
    secrets::{AppSecretManager, SecretsManagerConfig},
    storage::{StorageLayerFactory, StorageLayerFactoryConfig},
    tenant::tenant_cache::TenantCache,
};
use docbox_database::DatabasePoolCache;
use docbox_search::{SearchIndexFactory, SearchIndexFactoryConfig};
use docbox_web_scraper::{WebsiteMetaService, WebsiteMetaServiceConfig};
use logging::{init_logging, init_logging_with_sentry};
use routes::router;
use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    sync::Arc,
};
use tower_http::{limit::RequestBodyLimitLayer, trace::TraceLayer};
use tracing::debug;

mod docs;
mod error;
mod logging;
mod middleware;
mod models;
pub mod routes;

// Current size limit 100MB, adjust according to our decided max size
const MAX_FILE_SIZE: usize = 100 * 1000 * 1024;

/// Default server address when not specified
const DEFAULT_SERVER_ADDRESS: SocketAddr =
    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 8080));

fn main() -> anyhow::Result<()> {
    _ = dotenvy::dotenv();

    let _sentry_guard = match std::env::var("SENTRY_DSN") {
        // Initialize logging with sentry support
        Ok(dsn) => {
            let sentry = init_logging_with_sentry(dsn)?;
            Some(sentry)
        }
        // Initialize logging without sentry support
        Err(_) => {
            init_logging()?;
            None
        }
    };

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed building the Runtime")
        .block_on(server())
}

async fn server() -> anyhow::Result<()> {
    // Create the converter
    let convert_server_addresses =
        std::env::var("CONVERT_SERVER_ADDRESS").unwrap_or("http://127.0.0.1:8081".to_string());
    let converter_server =
        OfficeConverterServer::from_addresses(convert_server_addresses.split(','))?;
    let converter = OfficeConverter::ConverterServer(converter_server);

    // Setup processing layer data
    let processing = ProcessingLayer {
        office: OfficeProcessingLayer { converter },
    };

    // Create website scraping service
    let website_meta_service_config =
        WebsiteMetaServiceConfig::from_env().context("failed to derive web scraper config")?;
    let website_meta_service = Arc::new(
        WebsiteMetaService::from_config(website_meta_service_config)
            .context("failed to build web scraper http client")?,
    );

    // Load AWS configuration
    let aws_config = aws_config().await;

    // Create secrets manager
    let secrets_config = SecretsManagerConfig::from_env()?;
    let secrets = AppSecretManager::from_config(&aws_config, secrets_config);

    // Load database credentials
    let db_host: String =
        std::env::var("POSTGRES_HOST").context("missing environment variable POSTGRES_HOST")?;
    let db_port: u16 = std::env::var("POSTGRES_PORT")
        .context("missing environment variable POSTGRES_PORT")?
        .parse()
        .context("invalid POSTGRES_PORT port value")?;
    let db_root_secret_name = std::env::var("DOCBOX_DB_CREDENTIAL_NAME")
        .context("missing environment variable DOCBOX_DB_CREDENTIAL_NAME")?;

    // Setup database cache / connector
    let db_cache = DatabasePoolCache::new(db_host, db_port, db_root_secret_name, secrets);
    let db_cache = Arc::new(db_cache);

    // Create the SQS client
    // Warning: Will panic if the configuration provided is invalid
    let sqs_client = SqsClient::new(&aws_config);

    // Setup event publisher factories
    let sqs_publisher_factory = SqsEventPublisherFactory::new(sqs_client.clone());
    let event_publisher_factory = EventPublisherFactory::new(sqs_publisher_factory);

    // Setup search index factory
    let search_config = SearchIndexFactoryConfig::from_env()?;
    let search_index_factory = SearchIndexFactory::from_config(&aws_config, search_config)?;

    // Setup storage factory
    let storage_factory_config = StorageLayerFactoryConfig::from_env()?;
    let storage_factory = StorageLayerFactory::from_config(&aws_config, storage_factory_config);

    // Create tenant cache
    let tenant_cache = Arc::new(TenantCache::new());

    // Setup notification queue
    let notification_config = NotificationConfig::from_env()?;
    let mut notification_queue =
        AppNotificationQueue::from_config(sqs_client, notification_config)?;

    // Setup router
    let mut app = router();

    if let AppNotificationQueue::Mpsc(queue) = &mut notification_queue {
        let sender = queue
            .take_sender()
            .context("missing sender for in memory notification queue")?;

        // Append the webhook handling endpoint and sender extension
        app = app
            .route("/webhook/s3", post(routes::utils::webhook_s3))
            .layer(Extension(sender));
    }

    // Spawn background task to process notification queue messages
    tokio::spawn(process_notification_queue(
        notification_queue,
        NotificationQueueData {
            db_cache: db_cache.clone(),
            search: search_index_factory.clone(),
            storage: storage_factory.clone(),
            events: event_publisher_factory.clone(),
            processing: processing.clone(),
        },
    ));

    // Spawn background scheduled tasks
    tokio::spawn(perform_background_tasks(BackgroundTaskData {
        db_cache: db_cache.clone(),
        storage: storage_factory.clone(),
    }));

    // Determine the socket address to bind against
    let server_address = std::env::var("SERVER_ADDRESS")
        .ok()
        .and_then(|value| value.parse::<SocketAddr>().ok())
        .unwrap_or(DEFAULT_SERVER_ADDRESS);

    // Setup app layers and extension
    let app = app
        .layer(Extension(search_index_factory))
        .layer(Extension(storage_factory))
        .layer(Extension(db_cache))
        .layer(Extension(website_meta_service))
        .layer(Extension(event_publisher_factory))
        .layer(Extension(processing))
        .layer(Extension(tenant_cache))
        .layer(DefaultBodyLimit::disable())
        .layer(RequestBodyLimitLayer::new(MAX_FILE_SIZE))
        .layer(TraceLayer::new_for_http());

    // Development mode CORS access for local browser testing
    #[cfg(debug_assertions)]
    let app = app.layer(tower_http::cors::CorsLayer::very_permissive());

    // Bind the TCP listener for the HTTP server
    let listener = tokio::net::TcpListener::bind(server_address).await?;

    // Log the startup message
    debug!("server started on {server_address}");

    // Serve the app
    axum::serve(listener, app)
        // Attach graceful shutdown to the shutdown receiver
        .with_graceful_shutdown(async move {
            _ = tokio::signal::ctrl_c().await;
        })
        .await?;

    Ok(())
}
