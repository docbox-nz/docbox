#![forbid(unsafe_code)]

use crate::{
    background::{BackgroundTaskData, perform_background_tasks},
    extensions::max_file_size::MaxFileSizeBytes,
    middleware::api_key::ApiKeyLayer,
    notifications::{
        AppNotificationQueue, NotificationConfig,
        process::{NotificationQueueData, process_notification_queue},
    },
};
use axum::{Extension, extract::DefaultBodyLimit, routing::post};
use axum_server::tls_rustls::RustlsConfig;
use cache::website_metadata::{CachingWebsiteMetaService, CachingWebsiteMetaServiceConfig};
use docbox_core::{
    aws::{SqsClient, aws_config},
    events::{EventPublisherFactory, sqs::SqsEventPublisherFactory},
    tenant::tenant_cache::TenantCache,
};
use docbox_database::{DatabasePoolCache, DatabasePoolCacheConfig};
use docbox_processing::{
    ProcessingLayer, ProcessingLayerConfig,
    office::{OfficeConverter, OfficeConverterConfig, OfficeProcessingLayer},
};
use docbox_search::{SearchIndexFactory, SearchIndexFactoryConfig};
use docbox_secrets::{SecretManager, SecretsManagerConfig};
use docbox_storage::{StorageLayerFactory, StorageLayerFactoryConfig};
use docbox_web_scraper::{WebsiteMetaService, WebsiteMetaServiceConfig};
use logging::{init_logging, init_logging_with_sentry};
use routes::router;
use std::{
    error::Error,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    sync::Arc,
};
use tower_http::{limit::RequestBodyLimitLayer, trace::TraceLayer};
use tracing::debug;

mod background;
mod cache;
mod docs;
mod error;
mod extensions;
mod logging;
mod middleware;
mod models;
mod notifications;
pub mod routes;

/// The server version extracted from the Cargo.toml
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default server address when not specified (HTTP)
const DEFAULT_SERVER_ADDRESS_HTTP: SocketAddr =
    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 8080));

/// Default server address when not specified (HTTPS)
const DEFAULT_SERVER_ADDRESS_HTTPS: SocketAddr =
    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 8443));

fn main() -> Result<(), Box<dyn Error>> {
    _ = dotenvy::dotenv();

    let _sentry_guard = match std::env::var("SENTRY_DSN") {
        // Initialize logging with sentry support
        Ok(dsn) => {
            let sentry = init_logging_with_sentry(dsn);
            Some(sentry)
        }
        // Initialize logging without sentry support
        Err(_) => {
            init_logging();
            None
        }
    };

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed building the Runtime")
        .block_on(async move {
            if let Err(error) = server().await {
                tracing::error!(?error, message = %error, "error running server");
                return Err(error);
            }

            Ok(())
        })
}

async fn server() -> Result<(), Box<dyn Error>> {
    let max_file_size_bytes = match std::env::var("DOCBOX_MAX_FILE_SIZE_BYTES") {
        Ok(value) => value.parse::<i32>()?,
        // Default max file size in bytes (100MB)
        Err(_) => 100 * 1000 * 1024,
    };

    // Load AWS configuration
    let aws_config = aws_config().await;

    // Create website scraping service
    let website_meta_service_config = WebsiteMetaServiceConfig::from_env()?;
    let website_meta_service = WebsiteMetaService::from_config(website_meta_service_config)?;
    let caching_website_meta_service_config = CachingWebsiteMetaServiceConfig::from_env()?;
    let caching_website_meta_service =
        Arc::new(CachingWebsiteMetaService::from_client_with_config(
            website_meta_service,
            caching_website_meta_service_config,
        ));

    // Create secrets manager
    let secrets_config = SecretsManagerConfig::from_env()?;
    let secrets = SecretManager::from_config(&aws_config, secrets_config);

    // Load database credentials
    let db_pool_config = DatabasePoolCacheConfig::from_env()?;

    // API key
    let api_key = std::env::var("DOCBOX_API_KEY").ok();

    // Setup database cache / connector
    let db_cache = Arc::new(DatabasePoolCache::from_config(
        db_pool_config,
        secrets.clone(),
    ));

    // Create the SQS client
    // Warning: Will panic if the configuration provided is invalid
    let sqs_client = SqsClient::new(&aws_config);

    // Setup event publisher factories
    let sqs_publisher_factory = SqsEventPublisherFactory::new(sqs_client.clone());
    let event_publisher_factory = EventPublisherFactory::new(sqs_publisher_factory);

    // Setup search index factory
    let search_config = SearchIndexFactoryConfig::from_env()?;
    let search_index_factory =
        SearchIndexFactory::from_config(&aws_config, secrets, db_cache.clone(), search_config)?;

    // Setup storage factory
    let storage_factory_config = StorageLayerFactoryConfig::from_env()?;
    let storage_factory = StorageLayerFactory::from_config(&aws_config, storage_factory_config);

    // Create the converter
    let converter_config = OfficeConverterConfig::from_env()?;
    let converter = OfficeConverter::from_config(&aws_config, &storage_factory, converter_config)?;

    // Load the config for the processing layer
    let processing_layer_config = ProcessingLayerConfig::from_env()?;

    // Setup processing layer
    let processing = ProcessingLayer {
        office: OfficeProcessingLayer { converter },
        config: processing_layer_config,
    };

    // Create tenant cache
    let tenant_cache = Arc::new(TenantCache::new());

    // Setup notification queue
    let notification_config = NotificationConfig::from_env();
    let mut notification_queue = AppNotificationQueue::from_config(sqs_client, notification_config);

    // Setup router
    let mut app = router();

    if let AppNotificationQueue::Mpsc(queue) = &mut notification_queue {
        let sender = queue.take_sender().ok_or_else(|| {
            std::io::Error::other("missing sender for in memory notification queue")
        })?;

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

    // When operating in an environment where multiple servers are running we may want to
    // disable automated background tasks that could run concurrently and interfere with
    // each other
    let disable_background_tasks = match std::env::var("DOCBOX_DISABLE_BACKGROUND_TASKS") {
        Ok(value) => value.parse::<bool>()?,
        // Default max file size in bytes (100MB)
        Err(_) => false,
    };

    if disable_background_tasks {
        tracing::debug!("background tasks are disabled, skipping schedule");
    } else {
        tracing::debug!("scheduling background tasks");

        // Spawn background scheduled tasks
        tokio::spawn(perform_background_tasks(BackgroundTaskData {
            db_cache: db_cache.clone(),
            storage: storage_factory.clone(),
        }));
    }

    // Determine whether to use https
    let use_https = match std::env::var("DOCBOX_USE_HTTPS") {
        Ok(value) => value.parse::<bool>()?,
        // Default max file size in bytes (100MB)
        Err(_) => false,
    };

    // Determine the socket address to bind against
    let server_address = std::env::var("SERVER_ADDRESS")
        .ok()
        .and_then(|value| value.parse::<SocketAddr>().ok())
        .unwrap_or(if use_https {
            DEFAULT_SERVER_ADDRESS_HTTPS
        } else {
            DEFAULT_SERVER_ADDRESS_HTTP
        });

    // Setup app layers and extension
    let mut app = app
        .layer(Extension(search_index_factory))
        .layer(Extension(storage_factory))
        .layer(Extension(db_cache.clone()))
        .layer(Extension(caching_website_meta_service))
        .layer(Extension(event_publisher_factory))
        .layer(Extension(processing))
        .layer(Extension(tenant_cache))
        .layer(Extension(MaxFileSizeBytes(max_file_size_bytes)))
        .layer(DefaultBodyLimit::disable())
        .layer(RequestBodyLimitLayer::new(max_file_size_bytes as usize))
        .layer(TraceLayer::new_for_http());

    if let Some(api_key) = api_key {
        app = app.layer(ApiKeyLayer::new(api_key));
    } else {
        tracing::warn!(
            "DOCBOX_API_KEY not specified, its recommended you set one for security reasons"
        )
    }

    // Development mode CORS access for local browser testing
    #[cfg(debug_assertions)]
    let app = app.layer(tower_http::cors::CorsLayer::very_permissive());

    // Log the startup message
    debug!("server started on {server_address}");

    let handle = axum_server::Handle::default();

    // Handle graceful shutdown on CTRL+C
    tokio::spawn({
        let handle = handle.clone();
        async move {
            _ = tokio::signal::ctrl_c().await;
            handle.graceful_shutdown(None);

            db_cache.close_all().await;
        }
    });

    if use_https {
        // Determine whether to use https
        let certificate_path = match std::env::var("DOCBOX_HTTPS_CERTIFICATE_PATH") {
            Ok(value) => value,
            Err(_) => "docbox.cert.pem".to_string(),
        };

        let private_key_path = match std::env::var("DOCBOX_HTTPS_PRIVATE_KEY_PATH") {
            Ok(value) => value,
            Err(_) => "docbox.key.pem".to_string(),
        };

        if rustls::crypto::aws_lc_rs::default_provider()
            .install_default()
            .is_err()
        {
            tracing::error!("failed install default crypto provider");
            return Err(std::io::Error::other("failed to install default crypto provider").into());
        }

        let config = match RustlsConfig::from_pem_file(certificate_path, private_key_path).await {
            Ok(value) => value,
            Err(error) => {
                tracing::error!(?error, "failed to initialize https config");
                return Err(error.into());
            }
        };

        // Serve the app over HTTPS
        axum_server::bind_rustls(server_address, config)
            .handle(handle)
            .serve(app.into_make_service())
            .await?;
    } else {
        // Serve the app over HTTP
        axum_server::bind(server_address)
            .handle(handle)
            .serve(app.into_make_service())
            .await?;
    }

    Ok(())
}
