use anyhow::Context;
use axum::{extract::DefaultBodyLimit, Extension};
use docbox_core::{
    aws::{aws_config, create_s3_client_dev, S3Client, SecretsManagerClient, SqsClient},
    background::perform_background_tasks,
    events::{sqs::SqsEventPublisherFactory, EventPublisherFactory},
    notifications::{process_notification_queue, AppNotificationQueue},
    processing::{office::OfficeProcessingLayer, ProcessingLayer},
    search::{
        os::{create_open_search, OpenSearchIndexFactory},
        SearchIndexFactory,
    },
    secrets::{aws::AwsSecretManager, memory::MemorySecretManager, AppSecretManager, Secret},
    services::pdf::LibreOfficeConverter,
    storage::{s3::S3StorageLayerFactory, StorageLayerFactory},
};
use docbox_database::DatabasePoolCache;
use docbox_web_scraper::WebsiteMetaService;
use routes::router;
use serde_json::json;
use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    sync::Arc,
};
use tower_http::{limit::RequestBodyLimitLayer, trace::TraceLayer};
use tracing::debug;
use tracing_subscriber::EnvFilter;

mod error;
mod middleware;
mod models;
mod routes;

// Current size limit 100MB, adjust according to our decided max size
const MAX_FILE_SIZE: usize = 100 * 1000 * 1024;

/// Environment variable to use for the server address
const SERVER_ADDRESS_ENV: &str = "SERVER_ADDRESS";

/// Default server address when not specified
const DEFAULT_SERVER_ADDRESS: SocketAddr =
    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 8080));

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    _ = dotenvy::dotenv();

    // Start configuring a `fmt` subscriber
    let subscriber = tracing_subscriber::fmt()
        // Use the logging options from env variables
        .with_env_filter(EnvFilter::from_default_env())
        // Display source code file paths
        .with_file(true)
        // Display source code line numbers
        .with_line_number(true)
        // Don't display the event's target (module path)
        .with_target(false)
        // Build the subscriber
        .finish();

    // use that subscriber to process traces emitted after this point
    tracing::subscriber::set_global_default(subscriber)?;

    // Create the converter
    let converter = LibreOfficeConverter::init()?;

    // Setup processing layer data
    let processing = ProcessingLayer {
        office: OfficeProcessingLayer { converter },
    };

    // Create website scraping service
    let website_meta_service = Arc::new(WebsiteMetaService::new()?);

    // Load AWS configuration
    let aws_config = aws_config().await;

    // Create secrets manager
    let secrets = match cfg!(debug_assertions) {
        true => {
            // Running locally in debug mode targeting a local database
            // uses the same credentials for all secrets
            let username = std::env::var("POSTGRES_USER")
                .context("missing environment variable POSTGRES_USER")?;
            let password = std::env::var("POSTGRES_PASSWORD")
                .context("missing environment variable POSTGRES_PASSWORD")?;

            let value = serde_json::to_string(&json!({
                "username": username,
                "password": password,
            }))
            .context("failed to encode database secret")?;

            AppSecretManager::Memory(MemorySecretManager::new(
                Default::default(),
                Some(Secret::String(value)),
            ))
        }
        false => {
            let client = SecretsManagerClient::new(&aws_config);
            AppSecretManager::Aws(AwsSecretManager::new(client))
        }
    };

    // Setup database cache / connector
    let db_cache = DatabasePoolCache::new(secrets);
    let db_cache = Arc::new(db_cache);

    // Setup opensearch
    let open_search = create_open_search(&aws_config).context("failed to create open search")?;

    // Setup S3 client
    let s3_client = match cfg!(debug_assertions) {
        true => create_s3_client_dev(),
        false => S3Client::new(&aws_config),
    };

    // Create the SQS client
    // Warning: Will panic if the configuration provided is invalid
    let sqs_client = SqsClient::new(&aws_config);

    // Setup event publisher factories
    let sqs_publisher_factory = SqsEventPublisherFactory::new(sqs_client.clone());
    let event_publisher_factory = EventPublisherFactory::new(sqs_publisher_factory);

    // Setup search index factories
    let os_index_factory = OpenSearchIndexFactory::new(open_search);
    let search_index_factory = SearchIndexFactory::new(os_index_factory);

    let s3_storage_factory = S3StorageLayerFactory::new(s3_client);
    let storage_factory = StorageLayerFactory::new(s3_storage_factory);

    // Setup notification queue
    let notification_queue = match std::env::var("DOCBOX_SQS_URL") {
        Ok(notification_queue_url) => {
            tracing::debug!(queue_url = %notification_queue_url, "using SQS notification queue");
            AppNotificationQueue::create_sqs(sqs_client, notification_queue_url)
        }
        Err(cause) => {
            tracing::warn!(
                ?cause,
                "DOCBOX_SQS_URL queue not specified, falling back to no-op queue"
            );
            AppNotificationQueue::create_noop()
        }
    };

    // Spawn background task to process notification queue messages
    tokio::spawn(process_notification_queue(
        notification_queue,
        db_cache.clone(),
        search_index_factory.clone(),
        storage_factory.clone(),
        event_publisher_factory.clone(),
        processing.clone(),
    ));

    // Spawn background scheduled tasks
    tokio::spawn(perform_background_tasks(
        db_cache.clone(),
        storage_factory.clone(),
    ));

    // Determine the socket address to bind against
    let server_address = std::env::var(SERVER_ADDRESS_ENV)
        .context("missing or invalid server address")
        .and_then(|value| {
            value
                .parse::<SocketAddr>()
                .context("SERVER_ADDRESS was not a valid socket address")
        })
        .unwrap_or(DEFAULT_SERVER_ADDRESS);

    let app = router()
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
    let listener = tokio::net::TcpListener::bind(server_address).await.unwrap();

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
