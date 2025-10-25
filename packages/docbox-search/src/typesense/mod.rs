use crate::{
    SearchError, SearchIndex,
    models::{
        DocumentPage, FileSearchRequest, FileSearchResults, FlattenedItemResult, PageResult,
        SearchIndexData, SearchRequest, SearchResults, SearchScore, UpdateSearchIndexData,
    },
    typesense::{
        api_key::ApiKeyProvider,
        models::{
            GenericSearchResponse, GroupedSearchResponse, SearchResponse, TypesenseDataEntry,
            TypesenseDataEntryPageV1, TypesenseDataEntryRootV1, TypesenseDataEntryV1,
            TypesenseEntry,
        },
    },
};
use docbox_database::{
    DbTransaction,
    models::{
        document_box::{DocumentBoxScopeRaw, DocumentBoxScopeRawRef},
        file::FileId,
        folder::FolderId,
        tenant::Tenant,
    },
};
use docbox_secrets::SecretManager;
use itertools::Itertools;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{fmt::Debug, sync::Arc};
use uuid::Uuid;

pub use api_key::{TypesenseApiKey, TypesenseApiKeyProvider, TypesenseApiKeySecret};
pub use error::{TypesenseIndexFactoryError, TypesenseSearchError};

pub mod api_key;
pub mod error;
mod models;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TypesenseSearchConfig {
    pub url: String,

    /// Config provides the API key directly
    pub api_key: Option<TypesenseApiKey>,

    /// Config provides a secret manager key pointing to the API key
    pub api_key_secret_name: Option<String>,
}

impl TypesenseSearchConfig {
    pub fn from_env() -> Result<Self, TypesenseIndexFactoryError> {
        let url =
            std::env::var("TYPESENSE_URL").map_err(|_| TypesenseIndexFactoryError::MissingUrl)?;

        let api_key = std::env::var("TYPESENSE_API_KEY")
            .map(TypesenseApiKey::new)
            .ok();
        let api_key_secret_name = std::env::var("TYPESENSE_API_KEY_SECRET_NAME").ok();

        Ok(Self {
            url,
            api_key,
            api_key_secret_name,
        })
    }
}

/// Shared client data (Base URL, API Key provider, ..etc)
pub struct TypesenseClientData {
    base_url: String,
    api_key_provider: TypesenseApiKeyProvider,
}

#[derive(Clone)]
pub struct TypesenseIndexFactory {
    client: reqwest::Client,
    client_data: Arc<TypesenseClientData>,
}

impl TypesenseIndexFactory {
    pub fn from_config(
        secrets: SecretManager,
        config: TypesenseSearchConfig,
    ) -> Result<Self, TypesenseIndexFactoryError> {
        let api_key_provider = match (config.api_key, config.api_key_secret_name) {
            (Some(api_key), _) => {
                tracing::debug!("using typesense api key");
                TypesenseApiKeyProvider::ApiKey(api_key)
            }
            (_, Some(secret_name)) => {
                tracing::debug!("using secret manager controller typesense api key");
                TypesenseApiKeyProvider::Secret(TypesenseApiKeySecret::new(secrets, secret_name))
            }
            _ => return Err(TypesenseIndexFactoryError::MissingApiKey),
        };

        let client = reqwest::Client::builder()
            // Don't try and proxy through the proxy
            .no_proxy()
            .build()
            .map_err(|error| {
                tracing::error!(?error, " failed to create typesense http client");
                TypesenseIndexFactoryError::CreateClient
            })?;

        let client_data = Arc::new(TypesenseClientData {
            base_url: config.url,
            api_key_provider,
        });

        Ok(Self {
            client,
            client_data,
        })
    }

    pub fn create_search_index(&self, index: String) -> TypesenseIndex {
        TypesenseIndex {
            client: self.client.clone(),
            client_data: self.client_data.clone(),
            index,
        }
    }
}

#[derive(Clone)]
pub struct TypesenseIndex {
    client: reqwest::Client,
    client_data: Arc<TypesenseClientData>,
    index: String,
}

fn escape_typesense_value(input: &str) -> String {
    // Escape backticks within the text
    let escaped = input.replace('`', "\\`");

    // Surround the text with backticks
    format!("`{escaped}`")
}

impl SearchIndex for TypesenseIndex {
    async fn create_index(&self) -> Result<(), SearchError> {
        let api_key = self.client_data.api_key_provider.get_api_key().await?;

        let schema = json!({
          "name": self.index,
          "fields": [
            { "name": "id", "type": "string" },

            { "name": "version", "type": "string", "facet": true },
            { "name": "entry_type", "type": "string", "facet": true },

            { "name": "document_box", "type": "string", "facet": true },
            { "name": "folder_id", "type": "string", "facet": true },

            { "name": "item_type", "type": "string", "facet": true },
            { "name": "item_id", "type": "string", "facet": true },
            { "name": "name", "type": "string" },

            { "name": "value", "type": "string", "optional": true },
            { "name": "mime", "type": "string", "optional": true },

            { "name": "created_at", "type": "int64", "facet": true },
            { "name": "created_by", "type": "string", "optional": true, "facet": true },

            { "name": "page", "type": "int32", "optional": true },
            { "name": "page_content", "type": "string", "optional": true }
          ]
        });

        self.client
            .post(format!("{}/collections", self.client_data.base_url))
            .header("x-typesense-api-key", api_key)
            .json(&schema)
            .send()
            .await
            .map_err(|error| {
                tracing::error!(?error, "failed to create search index (io)");
                TypesenseSearchError::CreateIndex
            })?
            .error_for_status()
            .map_err(|error| {
                tracing::error!(?error, "failed to create search index (response)");
                TypesenseSearchError::CreateIndex
            })?;

        Ok(())
    }

    async fn index_exists(&self) -> Result<bool, SearchError> {
        let api_key = self.client_data.api_key_provider.get_api_key().await?;

        let response = self
            .client
            .get(format!(
                "{}/collections/{}",
                self.client_data.base_url, self.index
            ))
            .header("x-typesense-api-key", api_key)
            .send()
            .await
            .map_err(|error| {
                tracing::error!(?error, "failed to get search index (io)");
                TypesenseSearchError::GetIndex
            })?;

        if response.status() == StatusCode::NOT_FOUND {
            return Ok(false);
        }

        response.error_for_status().map_err(|error| {
            tracing::error!(?error, "failed to get search index (response)");
            TypesenseSearchError::GetIndex
        })?;

        Ok(true)
    }

    async fn delete_index(&self) -> Result<(), SearchError> {
        let api_key = self.client_data.api_key_provider.get_api_key().await?;

        self.client
            .delete(format!(
                "{}/collections/{}",
                self.client_data.base_url, self.index
            ))
            .header("x-typesense-api-key", api_key)
            .send()
            .await
            .map_err(|error| {
                tracing::error!(?error, "failed to delete search index (io)");
                TypesenseSearchError::DeleteIndex
            })?
            .error_for_status()
            .map_err(|error| {
                tracing::error!(?error, "failed to delete search index (response)");
                TypesenseSearchError::DeleteIndex
            })?;

        // TODO: Gracefully handle 404 from already deleted index

        Ok(())
    }

    async fn search_index_file(
        &self,
        scope: &DocumentBoxScopeRaw,
        file_id: FileId,
        query: FileSearchRequest,
    ) -> Result<FileSearchResults, SearchError> {
        let api_key = self.client_data.api_key_provider.get_api_key().await?;

        let offset = query.offset.unwrap_or(0);
        let limit = query.limit.unwrap_or(50);
        let query = query.query.unwrap_or_default();
        let filter_by = format!(
            r#"document_box:="{}"&&item_id:="{}"&&entry_type:="Page""#,
            escape_typesense_value(scope),
            // UUID does not need to be escaped
            &file_id
        );

        let query_json = json!({
            "searches": [
                {
                    "collection": self.index,
                    "q": query,
                    "query_by": "page_content",
                    "offset": offset,
                    "limit": limit,
                    "filter_by": filter_by,
                    "exclude_fields": "page_content",
                    "highlight_fields": "page_content",
                    "highlight_start_tag": "<em>",
                    "highlight_end_tag": "</em>",
                    "highlight_affix_num_tokens": 15,
                }
            ]
        });

        let response = self
            .client
            .post(format!("{}/multi_search", self.client_data.base_url))
            .header("x-typesense-api-key", api_key)
            .json(&query_json)
            .send()
            .await
            .map_err(|error| {
                tracing::error!(?error, "failed to query typesense multi_search");
                TypesenseSearchError::SearchIndex
            })?;

        if let Err(error) = response.error_for_status_ref() {
            let body = response.text().await;
            tracing::error!(?error, ?body, "failed to get search results");
            return Err(TypesenseSearchError::SearchIndex.into());
        }

        let search: SearchResponse<GenericSearchResponse> =
            response.json().await.map_err(|error| {
                tracing::error!(?error, "failed to parse search response JSON");
                TypesenseSearchError::SearchIndex
            })?;

        let search = search
            .results
            .into_iter()
            .next()
            .ok_or(TypesenseSearchError::MissingSearchResult)?;

        let total_hits = search.found;
        let results: Vec<PageResult> = search
            .hits
            .into_iter()
            .filter_map(|hit| match hit.document {
                TypesenseDataEntry::V1(TypesenseDataEntryV1::Page(page)) => {
                    let highlighted = hit
                        .highlights
                        .into_iter()
                        .find(|value| value.field == "page_content")
                        .map(|value| value.snippet)?;

                    Some(PageResult {
                        page: page.page,
                        matches: vec![highlighted],
                    })
                }
                _ => None,
            })
            .collect();

        Ok(FileSearchResults {
            total_hits,
            results,
        })
    }

    async fn search_index(
        &self,
        scopes: &[DocumentBoxScopeRaw],
        query: SearchRequest,
        folder_children: Option<Vec<FolderId>>,
    ) -> Result<SearchResults, SearchError> {
        let mut query_by = Vec::new();

        // Query file name
        if query.include_name {
            query_by.push("name");
        }

        // Querying within content (link value and page content)
        if query.include_content {
            query_by.push("value");
            query_by.push("page_content");
        }

        let filter_by = create_search_filters(scopes, &query, folder_children);
        let search_query = query.query.unwrap_or_default();

        // Must query at least one field
        if query_by.is_empty() {
            // When specifying a query at least one field must be specified
            if !search_query.is_empty() && !filter_by.is_empty() {
                return Err(TypesenseSearchError::MissingQueryBy.into());
            }

            // For facet only queries the name is used as a dummy value
            query_by.push("name");
        }

        let has_wildcard = scopes.iter().any(|scope| scope.ends_with('*'));

        let max_filter_by_candidates = if has_wildcard {
            // Allow filtering over a much larger set when selecting a wildcard scope
            10_000
        } else {
            4
        };

        let size = query.size.unwrap_or(50);
        let offset = query.offset.unwrap_or(0);

        let max_pages = query.max_pages.unwrap_or(3);

        let query_by = query_by.join(",");

        let query_json = json!({
            "searches": [
                {
                    "collection": self.index,
                    "q": search_query,
                    "query_by": query_by,
                    "group_by": "item_id",
                    "group_limit": max_pages,
                    "offset": offset,
                    "limit": size,
                    "filter_by": filter_by,
                    "exclude_fields": "page_content",
                    "highlight_fields": "name,value,page_content",
                    "highlight_start_tag": "<em>",
                    "highlight_end_tag": "</em>",
                    "max_filter_by_candidates": max_filter_by_candidates
                }
            ]
        });

        tracing::debug!(?query_json, "performing search query");

        let api_key = self.client_data.api_key_provider.get_api_key().await?;

        let response = self
            .client
            .post(format!("{}/multi_search", self.client_data.base_url))
            .header("x-typesense-api-key", api_key)
            .json(&query_json)
            .send()
            .await
            .map_err(|error| {
                tracing::error!(?error, "failed to query typesense multi_search");
                TypesenseSearchError::SearchIndex
            })?;

        if let Err(error) = response.error_for_status_ref() {
            let body = response.text().await;
            let query = serde_json::to_string(&query_json);
            tracing::error!(?error, ?body, ?query, "failed to get search results");
            return Err(TypesenseSearchError::SearchIndex.into());
        }

        let response: serde_json::Value = response.json().await.map_err(|error| {
            tracing::error!(?error, "failed to parse search response JSON");
            TypesenseSearchError::SearchIndex
        })?;
        tracing::debug!(?response);

        let search: SearchResponse<GroupedSearchResponse> = serde_json::from_value(response)
            .map_err(|error| {
                tracing::error!(?error, "failed to parse search response JSON");
                TypesenseSearchError::SearchIndex
            })?;
        let search = search
            .results
            .into_iter()
            .next()
            .ok_or(TypesenseSearchError::MissingSearchResult)?;

        let total_hits = search.found;

        let results = search
            .grouped_hits
            .into_iter()
            .filter_map(|group| {
                let root = group.hits.first().map(|value| &value.document)?;

                match root {
                    TypesenseDataEntry::V1(TypesenseDataEntryV1::Root(root))
                    | TypesenseDataEntry::V1(TypesenseDataEntryV1::Page(
                        TypesenseDataEntryPageV1 { root, .. },
                    )) => {
                        let group_score = group
                            .hits
                            .iter()
                            .map(|hit| hit.text_match)
                            .max()
                            .unwrap_or_default();

                        // Check for name matches
                        let name_match = group.hits.iter().any(|hit| {
                            hit.highlights
                                .iter()
                                .any(|highlight| highlight.field == "name")
                        });

                        // Check for content matches
                        let content_match = group.hits.iter().any(|hit| {
                            hit.highlights.iter().any(|highlight| {
                                highlight.field == "value" || highlight.field == "page_content"
                            })
                        });

                        let page_matches: Vec<PageResult> = group
                            .hits
                            .iter()
                            .filter_map(|hit| match &hit.document {
                                TypesenseDataEntry::V1(TypesenseDataEntryV1::Page(page)) => {
                                    let highlighted = hit
                                        .highlights
                                        .iter()
                                        .find(|value| value.field == "page_content")
                                        .map(|value| value.snippet.to_string())?;

                                    Some(PageResult {
                                        page: page.page,
                                        matches: vec![highlighted],
                                    })
                                }
                                _ => None,
                            })
                            .collect();

                        Some(FlattenedItemResult {
                            item_ty: root.ty,
                            item_id: root.item_id,
                            document_box: root.document_box.clone(),
                            page_matches,
                            total_hits: group.found,
                            score: SearchScore::Integer(group_score),
                            name_match,
                            content_match,
                        })
                    }
                }
            })
            .collect();

        Ok(SearchResults {
            total_hits,
            results,
        })
    }

    async fn add_data(&self, data: Vec<SearchIndexData>) -> Result<(), SearchError> {
        let mut documents = Vec::new();

        for data in data {
            let root = TypesenseDataEntryRootV1 {
                document_box: data.document_box,
                folder_id: data.folder_id,
                ty: data.ty,
                item_id: data.item_id,
                created_at: data.created_at.timestamp(),
                created_by: data.created_by,
                value: data.content,
                mime: data.mime,
                name: data.name,
            };

            // When its a file with page data
            if let Some(pages) = data.pages {
                // Add a new entry for each page
                for page in pages {
                    documents.push(create_item_page(&root, page));
                }
            }

            // Create the root document
            documents.push(TypesenseEntry {
                id: Uuid::new_v4(),
                entry: TypesenseDataEntry::V1(TypesenseDataEntryV1::Root(root)),
            });
        }

        self.bulk_add_documents(documents).await?;

        Ok(())
    }

    async fn update_data(
        &self,
        item_id: Uuid,
        data: UpdateSearchIndexData,
    ) -> Result<(), SearchError> {
        // Update all the existing items so they have the current root data
        self.update_item_roots(item_id, &data).await?;

        if let Some(pages) = data.pages {
            // Delete all page based documents
            self.delete_item_pages(item_id).await?;

            // Resolve the root document
            let root = self
                .get_item_root(item_id)
                .await?
                .ok_or(TypesenseSearchError::MissingRootEntry)?;

            // Create an updated version of the root document to
            // use on the added pages
            let updated_root = TypesenseDataEntryRootV1 {
                document_box: root.document_box.clone(),
                folder_id: data.folder_id,
                ty: root.ty,
                item_id: root.item_id,
                name: data.name.clone(),
                value: data.content.clone(),
                mime: root.mime.clone(),
                created_at: root.created_at,
                created_by: root.created_by.clone(),
            };

            // Create the documents for the pages
            let documents = pages
                .into_iter()
                .map(|page| create_item_page(&updated_root, page))
                .collect();

            // Bulk insert the created documents
            self.bulk_add_documents(documents).await?;
        }

        Ok(())
    }

    async fn delete_data(&self, id: Uuid) -> Result<(), SearchError> {
        let api_key = self.client_data.api_key_provider.get_api_key().await?;

        self.client
            .delete(format!(
                "{}/collections/{}/documents",
                self.client_data.base_url, self.index
            ))
            .header("x-typesense-api-key", api_key)
            .query(&[("filter_by", format!(r#"item_id:="{id}""#))])
            .send()
            .await
            .map_err(|error| {
                tracing::error!(?error, "failed to delete data (request)");
                TypesenseSearchError::DeleteDocuments
            })?
            .error_for_status()
            .map_err(|error| {
                tracing::error!(?error, "failed to delete data (response)");
                TypesenseSearchError::DeleteDocuments
            })?;
        Ok(())
    }

    async fn delete_by_scope(&self, scope: DocumentBoxScopeRawRef<'_>) -> Result<(), SearchError> {
        let api_key = self.client_data.api_key_provider.get_api_key().await?;

        self.client
            .delete(format!(
                "{}/collections/{}/documents",
                self.client_data.base_url, self.index
            ))
            .header("x-typesense-api-key", api_key)
            .query(&[("filter_by", format!(r#"document_box:="{scope}""#))])
            .send()
            .await
            .map_err(|error| {
                tracing::error!(?error, "failed to delete data by scope (request)");
                eprintln!("{error:?}");
                TypesenseSearchError::DeleteDocuments
            })?
            .error_for_status()
            .map_err(|error| {
                tracing::error!(?error, "failed to delete data by scope (response)");
                eprintln!("{error:?}");
                TypesenseSearchError::DeleteDocuments
            })?;
        Ok(())
    }

    async fn get_pending_migrations(
        &self,
        _applied_names: Vec<String>,
    ) -> Result<Vec<String>, SearchError> {
        Ok(Vec::new())
    }

    async fn apply_migration(
        &self,
        _tenant: &Tenant,
        _root_t: &mut DbTransaction<'_>,
        _t: &mut DbTransaction<'_>,
        _name: &str,
    ) -> Result<(), SearchError> {
        Ok(())
    }
}

impl TypesenseIndex {
    /// Bulk insert typesense documents
    async fn bulk_add_documents(
        &self,
        entries: Vec<TypesenseEntry>,
    ) -> Result<(), TypesenseSearchError> {
        // Encode entries into newline delimitated encoded JSON strings
        let mut bulk_data = String::new();
        for document in entries {
            let value = serde_json::to_string(&document).map_err(|error| {
                tracing::error!(?error, "failed to serialize a document");
                TypesenseSearchError::BulkAddDocuments
            })?;
            bulk_data.push_str(&value);
            bulk_data.push('\n');
        }

        let api_key = self.client_data.api_key_provider.get_api_key().await?;

        self.client
            .post(format!(
                "{}/collections/{}/documents/import",
                self.client_data.base_url, self.index
            ))
            .header("x-typesense-api-key", api_key)
            .body(bulk_data)
            .send()
            .await
            .map_err(|error| {
                tracing::error!(?error, "failed to send import documents (request)");
                TypesenseSearchError::BulkAddDocuments
            })?
            .error_for_status()
            .map_err(|error| {
                tracing::error!(?error, "error status when importing documents (response)");
                TypesenseSearchError::BulkAddDocuments
            })?;

        Ok(())
    }

    /// Deletes all "Page" item types for a specific `item_id`
    async fn delete_item_pages(&self, item_id: Uuid) -> Result<(), TypesenseSearchError> {
        let api_key = self.client_data.api_key_provider.get_api_key().await?;

        self.client
            .delete(format!(
                "{}/collections/{}/documents",
                self.client_data.base_url, self.index
            ))
            .header("x-typesense-api-key", api_key)
            .query(&[(
                "filter_by",
                format!(r#"item_id:="{item_id}"&&entry_type="Page""#),
            )])
            .send()
            .await
            .map_err(|error| {
                tracing::error!(?error, "failed to send delete documents (request)");
                TypesenseSearchError::DeleteDocuments
            })?
            .error_for_status()
            .map_err(|error| {
                tracing::error!(?error, "failed to send delete documents (response)");
                TypesenseSearchError::DeleteDocuments
            })?;

        Ok(())
    }

    /// Updates the root portion of all documents for the provided item
    async fn update_item_roots(
        &self,
        item_id: Uuid,
        update: &UpdateSearchIndexData,
    ) -> Result<(), TypesenseSearchError> {
        let api_key = self.client_data.api_key_provider.get_api_key().await?;

        let request = json!({
            "folder_id": update.folder_id,
            "name": update.name,
            "value": update.content,
        });

        // Update all the existing items so they have the current root data
        self.client
            .patch(format!(
                "{}/collections/{}/documents",
                self.client_data.base_url, self.index
            ))
            .header("x-typesense-api-key", api_key)
            .query(&[("filter_by", format!(r#"item_id:="{item_id}""#))])
            .json(&request)
            .send()
            .await
            .map_err(|error| {
                tracing::error!(?error, "failed to send update documents (request)");
                TypesenseSearchError::UpdateDocument
            })?
            .error_for_status()
            .map_err(|error| {
                tracing::error!(?error, "failed to send update documents (response)");
                TypesenseSearchError::UpdateDocument
            })?;

        Ok(())
    }

    /// Get the "root" item document for the provided `item_id`
    async fn get_item_root(
        &self,
        item_id: Uuid,
    ) -> Result<Option<TypesenseDataEntryRootV1>, TypesenseSearchError> {
        let api_key = self.client_data.api_key_provider.get_api_key().await?;

        let response: GenericSearchResponse = self
            .client
            .get(format!(
                "{}/collections/{}/documents/search",
                self.client_data.base_url, self.index
            ))
            .header("x-typesense-api-key", api_key)
            .query(&[(
                "filter_by",
                format!(r#"item_id:="{item_id}"&&entry_type=Root"#),
            )])
            .send()
            .await
            .map_err(|error| {
                tracing::error!(?error, "failed to query documents search (request)");
                TypesenseSearchError::GetDocument
            })?
            .error_for_status()
            .map_err(|error| {
                tracing::error!(?error, "failed to query documents search (response)");
                TypesenseSearchError::GetDocument
            })?
            .json()
            .await
            .map_err(|error| {
                tracing::error!(?error, "failed to query documents search (response json)");
                TypesenseSearchError::GetDocument
            })?;

        let item = response
            .hits
            .into_iter()
            .filter_map(|hit| match hit.document {
                TypesenseDataEntry::V1(TypesenseDataEntryV1::Root(root)) => Some(root),
                _ => None,
            })
            .next();

        Ok(item)
    }
}

fn create_item_page(root: &TypesenseDataEntryRootV1, page: DocumentPage) -> TypesenseEntry {
    TypesenseEntry {
        id: Uuid::new_v4(),
        entry: TypesenseDataEntry::V1(TypesenseDataEntryV1::Page(TypesenseDataEntryPageV1 {
            root: root.clone(),
            page: page.page,
            page_content: Some(page.content),
        })),
    }
}

/// Create the required typesense search filters for the `query`
fn create_search_filters(
    scopes: &[DocumentBoxScopeRaw],
    query: &SearchRequest,
    folder_children: Option<Vec<FolderId>>,
) -> String {
    let mut filter_parts = Vec::new();

    // Add a filter for the required scopes
    {
        let scopes = scopes
            .iter()
            .map(|value| escape_typesense_value(value))
            .join(", ");

        filter_parts.push(format!("document_box:=[{scopes}]"));
    }

    // Filter to children of allowed folders
    if let Some(folder_children) = folder_children
        && !folder_children.is_empty()
    {
        let ids = folder_children
            .into_iter()
            // No need to escape UUIDs
            .map(|value| value.to_string())
            .join(", ");

        filter_parts.push(format!("folder_id:=[{ids}]"));
    }

    if let Some(range) = query.created_at.as_ref() {
        if let Some(start) = range.start {
            let start = start.timestamp();
            filter_parts.push(format!(r#"created_at:>{start}"#));
        }

        if let Some(end) = range.end {
            let end = end.timestamp();
            filter_parts.push(format!(r#"created_at:<{end}"#));
        }
    }

    if let Some(created_by) = query.created_by.as_ref() {
        filter_parts.push(format!(
            r#"created_by:="{}""#,
            // User ID's must be escaped
            escape_typesense_value(created_by)
        ));
    }

    if let Some(folder_id) = query.folder_id {
        filter_parts.push(format!(r#"folder_id:="{folder_id}""#));
    }

    filter_parts.join("&&")
}
