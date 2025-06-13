use docbox_core::{
    aws::aws_config,
    files::{index_file::store_file_index, upload_file::store_generated_files},
    processing::office::{OfficeConverter, convert_server::OfficeConverterServer},
    processing::{
        ProcessingIndexMetadata, ProcessingLayer, office::OfficeProcessingLayer, process_file,
    },
    secrets::AppSecretManager,
    storage::{StorageLayerFactory, TenantStorageLayer},
    utils::file::get_file_name_ext,
};
use docbox_database::{
    DatabasePoolCache, DbPool,
    models::{
        file::{File, FileWithScope},
        tenant::{Tenant, TenantId},
    },
};
use docbox_search::{SearchIndexFactory, TenantSearchIndex};
use eyre::{Context, ContextCompat};
use futures::{StreamExt, future::BoxFuture};
use mime::Mime;
use std::ops::DerefMut;

use crate::{AnyhowError, CliConfiguration};

pub async fn reprocess_octet_stream_files(
    config: &CliConfiguration,
    env: String,
    tenant_id: TenantId,
) -> eyre::Result<()> {
    tracing::debug!(?env, ?tenant_id, "rebuilding tenant index");

    // Load AWS configuration
    let aws_config = aws_config().await;

    // Connect to secrets manager
    let secrets = AppSecretManager::from_config(&aws_config, config.secrets.clone());

    // Setup database cache / connector
    let db_cache = DatabasePoolCache::new(
        config.database.host.clone(),
        config.database.port,
        config.database.root_secret_name.clone(),
        secrets,
    );

    let search_factory = SearchIndexFactory::from_config(&aws_config, config.search.clone())
        .map_err(|err| eyre::Error::msg(err.to_string()))?;

    // Setup S3 access
    let storage_factory = StorageLayerFactory::from_config(&aws_config, config.storage.clone());

    let root_db = db_cache.get_root_pool().await?;
    let tenant = Tenant::find_by_id(&root_db, tenant_id, &env)
        .await?
        .context("tenant not found")?;

    let db = db_cache.get_tenant_pool(&tenant).await?;
    let search = search_factory.create_search_index(&tenant);

    tracing::info!(?tenant, "started re-indexing tenant");

    _ = search.create_index().await;

    let files = get_files(&db).await?;
    let mut skipped = Vec::new();
    let mut processing_files = Vec::new();

    // Create the converter
    let convert_server_addresses =
        std::env::var("CONVERT_SERVER_ADDRESS").unwrap_or("http://localhost:8081".to_string());
    let converter_server =
        OfficeConverterServer::from_addresses(convert_server_addresses.split(','))
            .map_err(AnyhowError)?;
    let converter = OfficeConverter::ConverterServer(converter_server);

    // Setup processing layer data
    let processing = ProcessingLayer {
        office: OfficeProcessingLayer { converter },
    };

    for file in files {
        let guessed_mime = get_file_name_ext(&file.file.name).and_then(|ext| {
            let guesses = mime_guess::from_ext(&ext);
            guesses.first()
        });

        if let Some(mime) = guessed_mime {
            processing_files.push((file, mime));
        } else {
            skipped.push(file);
        }
    }

    // Process all the files
    _ = futures::stream::iter(processing_files)
        .map(|(file, mime)| -> BoxFuture<'static, ()> {
            let db = db.clone();
            let search = search_factory.create_search_index(&tenant);
            let storage = storage_factory.create_storage_layer(&tenant);
            let processing = processing.clone();

            Box::pin(async move {
                tracing::debug!(?file, "stating file");
                if let Err(error) =
                    perform_process_file(db, storage, search, processing, file, mime).await
                {
                    tracing::error!(?error, "failed to migrate file");
                };
            })
        })
        .buffered(FILE_PROCESS_SIZE)
        .collect::<Vec<()>>()
        .await;

    for skipped in skipped {
        tracing::debug!(file_id = %skipped.file.id, file_name = %skipped.file.name, "skipped file");
    }

    Ok(())
}

/// Size of each page to request from the database
const DATABASE_PAGE_SIZE: u64 = 1000;
/// Number of files to process in parallel
const FILE_PROCESS_SIZE: usize = 50;

pub async fn get_files(db: &DbPool) -> eyre::Result<Vec<FileWithScope>> {
    let mut page_index = 0;
    let mut data = Vec::new();

    loop {
        let mut files = File::all_by_mime(
            db,
            "application/octet-stream",
            page_index * DATABASE_PAGE_SIZE,
            DATABASE_PAGE_SIZE,
        )
        .await
        .with_context(|| format!("failed to load files page: {page_index}"))?;

        let is_end = (files.len() as u64) < DATABASE_PAGE_SIZE;

        data.append(&mut files);

        if is_end {
            break;
        }

        page_index += 1;
    }

    Ok(data)
}

pub async fn perform_process_file(
    db: DbPool,
    storage: TenantStorageLayer,
    search: TenantSearchIndex,
    processing: ProcessingLayer,
    mut file: FileWithScope,
    mime: Mime,
) -> eyre::Result<()> {
    // Start a database transaction
    let mut db = db.begin().await.map_err(|cause| {
        tracing::error!(?cause, "failed to begin transaction");
        eyre::eyre!("failed to begin transaction")
    })?;

    let bytes = storage
        .get_file(&file.file.file_key)
        .await
        .map_err(AnyhowError)?
        .collect_bytes()
        .await
        .map_err(AnyhowError)?;

    let processing_output = process_file(&None, &processing, bytes, &mime).await?;

    let mut index_metadata: Option<ProcessingIndexMetadata> = None;

    if let Some(processing_output) = processing_output {
        // Store the encryption state for encrypted files
        if processing_output.encrypted {
            tracing::debug!("marking file as encrypted");
            file.file = file.file.set_encrypted(db.deref_mut(), true).await?;
        }

        index_metadata = processing_output.index_metadata;

        let mut s3_upload_keys = Vec::new();

        tracing::debug!("uploading generated files");
        store_generated_files(
            &mut db,
            &storage,
            &file.file,
            &mut s3_upload_keys,
            processing_output.upload_queue,
        )
        .await?;
    }

    // Index the file in the search index
    tracing::debug!("indexing file contents");
    store_file_index(&search, &file.file, &file.scope, index_metadata).await?;

    file.file = file.file.set_mime(db.deref_mut(), mime.to_string()).await?;

    db.commit().await?;

    Ok(())
}
