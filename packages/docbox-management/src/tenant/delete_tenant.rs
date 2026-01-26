use crate::database::{DatabaseProvider, close_pool_on_drop};
use docbox_core::{
    database::{
        DbErr, DbSecrets, ROOT_DATABASE_NAME,
        create::{delete_database, delete_role},
        models::{
            document_box::DocumentBox,
            tenant::{Tenant, TenantId},
        },
        utils::DatabaseErrorExt,
    },
    document_box::delete_document_box::{DeleteDocumentBoxError, delete_document_box},
    events::{EventPublisherFactory, TenantEventPublisher},
    search::{SearchError, SearchIndexFactory, TenantSearchIndex},
    secrets::{SecretManager, SecretManagerError},
    storage::{StorageLayer, StorageLayerError, StorageLayerFactory},
    tenant::tenant_options_ext::TenantOptionsExt,
};
use futures::{StreamExt, TryStreamExt};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::Instrument;

#[derive(Debug, Error)]
pub enum DeleteTenantError {
    #[error(transparent)]
    Database(DbErr),

    #[error("tenant not found")]
    TenantNotFound,

    #[error("failed to delete tenant: {0}")]
    DeleteTenant(DbErr),

    #[error("cannot perform additional resource deletion without specifying delete_contents")]
    MissingDeleteContents,

    #[error("failed to delete document box: {0}")]
    DeleteDocumentBox(DeleteDocumentBoxError),

    #[error("failed to delete storage bucket: {0}")]
    DeleteBucket(StorageLayerError),

    #[error("failed to delete search index: {0}")]
    DeleteSearch(SearchError),

    #[error("failed to delete database: {0}")]
    DeleteDatabase(DbErr),

    #[error("failed to delete database user: {0}")]
    DeleteDatabaseRole(DbErr),

    #[error("failed to get database secret: {0}")]
    GetDatabaseSecret(SecretManagerError),

    #[error("failed to delete database secret: {0}")]
    DeleteDatabaseSecret(SecretManagerError),
}

/// Number of document boxes to load in each database round trip
const DOCUMENT_BOX_PAGE_SIZE: u64 = 100;

/// Number of document boxes to handle deleting at once
const DOCUMENT_BOX_BATCH_SIZE: usize = 20;

#[derive(Debug, Serialize, Deserialize)]
pub struct DeleteTenant {
    pub env: String,
    pub tenant_id: TenantId,
    pub options: DeleteTenantOptions,
}

/// Destructive options
///
/// Some changes made by enabling these flags can make recovering the tenant
/// impossible in the case that you want to revert the deletion
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DeleteTenantOptions {
    /// Whether to delete data stored within the tenant
    pub delete_contents: bool,
    /// Whether to delete the tenant storage bucket itself (Requires "delete_contents")
    pub delete_storage: bool,
    /// Whether to delete the tenant search index itself (Requires "delete_contents")
    pub delete_search: bool,
    /// Whether to delete the tenant database itself (Requires "delete_contents")
    pub delete_database: bool,
    /// Whether when using AWS secrets manager to immediately delete the secret
    /// or to allow it to be recoverable for a short period of time.
    ///
    /// Note: If the secret is not immediately deleted a new tenant will not be
    /// able to make use of this secret name until the 30day recovery window
    /// has ended.
    pub permanently_delete_secret: bool,
}

#[tracing::instrument(skip_all, fields(env, tenant_id))]
pub async fn delete_tenant(
    db_provider: &impl DatabaseProvider,
    search_factory: &SearchIndexFactory,
    storage_factory: &StorageLayerFactory,
    events: &EventPublisherFactory,
    secrets: &SecretManager,
    config: DeleteTenant,
) -> Result<(), DeleteTenantError> {
    let db_docbox = db_provider
        .connect(ROOT_DATABASE_NAME)
        .await
        .map_err(DeleteTenantError::Database)?;
    let _guard = close_pool_on_drop(&db_docbox);

    let tenant = Tenant::find_by_id(&db_docbox, config.tenant_id, &config.env)
        .await
        .map_err(DeleteTenantError::Database)?
        .ok_or(DeleteTenantError::TenantNotFound)?;

    let search = search_factory.create_search_index(&tenant);
    let storage = storage_factory.create_layer(tenant.storage_layer_options());
    let events = events.create_event_publisher(&tenant);

    let options = config.options;

    if options.delete_contents {
        delete_tenant_contents(db_provider, &search, &storage, &events, &tenant).await?;
    }

    if options.delete_storage {
        if !options.delete_contents {
            return Err(DeleteTenantError::MissingDeleteContents);
        }

        if let Err(error) = storage.delete_bucket().await {
            tracing::error!(?error, "failed to delete storage bucket");
            return Err(DeleteTenantError::DeleteBucket(error));
        }
    }

    if options.delete_search {
        if !options.delete_contents {
            return Err(DeleteTenantError::MissingDeleteContents);
        }

        if let Err(error) = search.delete_index().await {
            tracing::error!(?error, "failed to delete storage bucket");
            return Err(DeleteTenantError::DeleteSearch(error));
        }
    }

    // Database search index must be explicitly closed before performing database operations
    if let TenantSearchIndex::Database(database) = search {
        database.close().await;
    }

    if options.delete_database {
        if !options.delete_contents {
            return Err(DeleteTenantError::MissingDeleteContents);
        }

        if let Err(error) = delete_database(&db_docbox, &tenant.db_name).await {
            // Database already not existing is fine
            if !error.is_database_does_not_exist() {
                tracing::error!(?error, "failed to delete tenant database");
                return Err(DeleteTenantError::DeleteDatabase(error));
            }
        }

        if let Some(db_secret_name) = tenant.db_secret_name.as_ref() {
            let db_secret = match secrets.parsed_secret::<DbSecrets>(db_secret_name).await {
                Ok(value) => value,
                Err(error) => {
                    tracing::error!(?error, "failed to get tenant database secret");
                    return Err(DeleteTenantError::GetDatabaseSecret(error));
                }
            };

            if let Some(role) = db_secret {
                if let Err(error) = delete_role(&db_docbox, &role.username).await {
                    tracing::error!(?error, "failed to delete tenant database secret");
                    return Err(DeleteTenantError::DeleteDatabaseRole(error));
                }

                if let Err(error) = secrets
                    .delete_secret(db_secret_name, options.permanently_delete_secret)
                    .await
                {
                    tracing::error!(?error, "failed to delete tenant database secret");
                    return Err(DeleteTenantError::DeleteDatabaseSecret(error));
                }
            } else {
                tracing::debug!(
                    "tenant secret not present, tenant database must have been deleted or secret was lost"
                );
            }
        }
    }

    tenant
        .delete(&db_docbox)
        .await
        .map_err(DeleteTenantError::DeleteTenant)?;

    Ok(())
}

async fn delete_tenant_contents(
    db_provider: &impl DatabaseProvider,
    search: &TenantSearchIndex,
    storage: &StorageLayer,
    events: &TenantEventPublisher,
    tenant: &Tenant,
) -> Result<(), DeleteTenantError> {
    let tenant_db = match db_provider.connect(&tenant.db_name).await {
        Ok(db) => db,
        Err(error) => {
            // Database has already been deleted, there's nothing more we can do here.
            // This could be a sign that a previous deletion attempt already completed this
            // portion and failed at a later step
            if error.is_database_does_not_exist() {
                return Ok(());
            }

            tracing::error!(?error, "failed to connect to tenant database");
            return Err(DeleteTenantError::Database(error));
        }
    };

    let _guard = close_pool_on_drop(&tenant_db);

    // Iterate document boxes in batches of 100 and begin removing them
    loop {
        let document_boxes = match DocumentBox::query(&tenant_db, 0, DOCUMENT_BOX_PAGE_SIZE).await {
            Ok(value) => value,
            Err(error) => {
                tracing::error!(?error, "failed to query document boxes");
                return Err(DeleteTenantError::Database(error));
            }
        };

        if document_boxes.is_empty() {
            break;
        }

        let span = tracing::Span::current();

        // Process document box deletions in batches
        futures::stream::iter(document_boxes)
            .map(|document_box| {
                let span = span.clone();
                let tenant_db = &tenant_db;

                async move {
                    if let Err(error) =
                        delete_document_box(tenant_db, search, storage, events, &document_box.scope)
                            .await
                    {
                        tracing::error!(?error, ?document_box, "failed to delete document box");
                        return Err(DeleteTenantError::DeleteDocumentBox(error));
                    }

                    Ok(())
                }
                .instrument(span)
            })
            .buffered(DOCUMENT_BOX_BATCH_SIZE)
            .try_collect::<()>()
            .await?;
    }

    // Explicitly close on graceful completion
    tenant_db.close().await;

    Ok(())
}
