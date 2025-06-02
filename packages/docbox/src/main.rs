use anyhow::Context;
use axum::{extract::DefaultBodyLimit, routing::post, Extension};
use docbox_core::{
    aws::{aws_config, SqsClient},
    background::{perform_background_tasks, BackgroundTaskData},
    events::{sqs::SqsEventPublisherFactory, EventPublisherFactory},
    notifications::{process_notification_queue, AppNotificationQueue, NotificationQueueData},
    office::{convert_server::OfficeConverterServer, OfficeConverter},
    processing::{office::OfficeProcessingLayer, ProcessingLayer},
    secrets::{AppSecretManager, SecretsManagerConfig},
    storage::{StorageLayerFactory, StorageLayerFactoryConfig},
};
use docbox_database::DatabasePoolCache;
use docbox_search::{SearchIndexFactory, SearchIndexFactoryConfig};
use docbox_web_scraper::WebsiteMetaService;
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

/// Environment variable to use for the server address
const SERVER_ADDRESS_ENV: &str = "SERVER_ADDRESS";

/// Default server address when not specified
const DEFAULT_SERVER_ADDRESS: SocketAddr =
    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 8080));

/// Environment variable to use for the convert server address
const CONVERT_SERVER_ADDRESS_ENV: &str = "CONVERT_SERVER_ADDRESS";

const DEFAULT_CONVERT_SERVER_ADDRESS: &str = "http://localhost:8081";

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
    let convert_server_addresses = std::env::var(CONVERT_SERVER_ADDRESS_ENV)
        .unwrap_or(DEFAULT_CONVERT_SERVER_ADDRESS.to_string());
    let converter_server =
        OfficeConverterServer::from_addresses(convert_server_addresses.split(','))?;
    let converter = OfficeConverter::ConverterServer(converter_server);

    // Setup processing layer data
    let processing = ProcessingLayer {
        office: OfficeProcessingLayer { converter },
    };

    // Create website scraping service
    let website_meta_service =
        Arc::new(WebsiteMetaService::new().context("failed to build web scraper http client")?);

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

    // Setup router
    let mut app = router();

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

    // Setup notification queue
    let notification_queue = match (
        std::env::var("DOCBOX_MPSC_QUEUE"),
        std::env::var("DOCBOX_SQS_URL"),
    ) {
        (Ok(_), _) => {
            tracing::debug!("DOCBOX_MPSC_QUEUE is set using local webhook notification queue");
            let (queue, tx) = AppNotificationQueue::create_mpsc();

            // Append the webhook handling endpoint and sender extension
            app = app
                .route("/webhook/s3", post(routes::utils::webhook_s3))
                .layer(Extension(tx));

            queue
        }
        (_, Ok(notification_queue_url)) => {
            tracing::debug!(queue_url = %notification_queue_url, "using SQS notification queue");
            AppNotificationQueue::create_sqs(sqs_client, notification_queue_url)
        }
        _ => {
            tracing::warn!("queue not specified, falling back to no-op queue");
            AppNotificationQueue::create_noop()
        }
    };

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
    let server_address = std::env::var(SERVER_ADDRESS_ENV)
        .context("missing or invalid server address")
        .and_then(|value| {
            value
                .parse::<SocketAddr>()
                .context("SERVER_ADDRESS was not a valid socket address")
        })
        .unwrap_or(DEFAULT_SERVER_ADDRESS);

    // Setup app layers and extension
    let app = app
        .layer(Extension(search_index_factory))
        .layer(Extension(storage_factory))
        .layer(Extension(db_cache))
        .layer(Extension(website_meta_service))
        .layer(Extension(event_publisher_factory))
        .layer(Extension(processing))
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
