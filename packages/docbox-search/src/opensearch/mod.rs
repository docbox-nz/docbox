use crate::SearchError;
use crate::models::FileSearchRequest;
use crate::opensearch::models::{OsSearchIndexData, OsUpdateSearchIndexData, SearchResponse};

use super::models::{FlattenedItemResult, PageResult, SearchScore};
use super::{
    SearchIndex,
    models::{
        FileSearchResults, SearchIndexData, SearchRequest, SearchResults, UpdateSearchIndexData,
    },
};
use aws_config::SdkConfig;
use docbox_database::DbTransaction;
use docbox_database::models::file::FileId;
use docbox_database::models::{
    document_box::DocumentBoxScopeRaw, folder::FolderId, tenant::Tenant,
};
use opensearch::{
    DeleteByQueryParts, OpenSearch, SearchParts,
    http::{
        Url,
        request::JsonBody,
        transport::{SingleNodeConnectionPool, TransportBuilder},
    },
    indices::{IndicesCreateParts, IndicesDeleteParts},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_with::skip_serializing_none;
use uuid::Uuid;

pub use error::{OpenSearchIndexFactoryError, OpenSearchSearchError};

pub mod error;
mod models;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenSearchConfig {
    pub url: String,
}

impl OpenSearchConfig {
    pub fn from_env() -> Result<Self, OpenSearchIndexFactoryError> {
        let url =
            std::env::var("OPENSEARCH_URL").map_err(|_| OpenSearchIndexFactoryError::MissingUrl)?;
        Ok(Self { url })
    }
}

#[derive(Clone)]
pub struct OpenSearchIndexFactory {
    client: OpenSearch,
}

impl OpenSearchIndexFactory {
    pub fn from_config(
        aws_config: &SdkConfig,
        config: OpenSearchConfig,
    ) -> Result<Self, OpenSearchIndexFactoryError> {
        let url = reqwest::Url::parse(&config.url).map_err(|error| {
            tracing::error!(?error, "failed to parse opensearch url");
            OpenSearchIndexFactoryError::InvalidUrl
        })?;
        let client = create_open_search(aws_config, url)?;
        Ok(Self { client })
    }

    pub fn create_search_index(&self, search_index: TenantSearchIndexName) -> OpenSearchIndex {
        OpenSearchIndex {
            client: self.client.clone(),
            search_index,
        }
    }
}

#[derive(Clone)]
pub struct OpenSearchIndex {
    client: OpenSearch,
    search_index: TenantSearchIndexName,
}

/// Represents a search index name for a specific tenant
#[derive(Clone, Debug)]
pub struct TenantSearchIndexName(String);

impl TenantSearchIndexName {
    pub fn from_tenant(tenant: &Tenant) -> Self {
        Self(tenant.os_index_name.clone())
    }
}

/// Create instance of [OpenSearch] from the environment
pub fn create_open_search(
    aws_config: &SdkConfig,
    url: Url,
) -> Result<OpenSearch, OpenSearchIndexFactoryError> {
    if cfg!(debug_assertions) {
        create_open_search_dev(url)
    } else {
        create_open_search_prod(aws_config, url)
    }
}

/// Create instance of [OpenSearch] from the environment
pub fn create_open_search_dev(url: Url) -> Result<OpenSearch, OpenSearchIndexFactoryError> {
    let conn_pool = SingleNodeConnectionPool::new(url);

    let transport = TransportBuilder::new(conn_pool)
        // We don't want open search trying to access the index through our proxy. It has a direct route
        .disable_proxy()
        // Disable certificate validation for local development
        .cert_validation(opensearch::cert::CertificateValidation::None)
        .build()
        .map_err(|error| {
            tracing::error!(?error, "failed to build opensearch transport");
            OpenSearchIndexFactoryError::BuildTransport
        })?;

    let open_search = OpenSearch::new(transport);

    Ok(open_search)
}

/// Create instance of [OpenSearch] from the environment
pub fn create_open_search_prod(
    aws_config: &SdkConfig,
    url: Url,
) -> Result<OpenSearch, OpenSearchIndexFactoryError> {
    // Setup opensearch connection pool
    let conn_pool = SingleNodeConnectionPool::new(url);

    let transport = TransportBuilder::new(conn_pool)
        // We don't want open search trying to access the index through our proxy. It has a direct route
        .disable_proxy()
        .auth(aws_config.clone().try_into().map_err(|error| {
            tracing::error!(?error, "failed to create opensearch transport auth config");
            OpenSearchIndexFactoryError::CreateAuthConfig
        })?)
        .service_name("es")
        .build()
        .map_err(|error| {
            tracing::error!(?error, "failed to build opensearch transport");
            OpenSearchIndexFactoryError::BuildTransport
        })?;

    let open_search = OpenSearch::new(transport);

    Ok(open_search)
}

impl SearchIndex for OpenSearchIndex {
    async fn create_index(&self) -> Result<(), SearchError> {
        // Create index for files
        let response = self
            .client
            .indices()
            .create(IndicesCreateParts::Index(&self.search_index.0))
            .body(json!({
                "settings": {
                    "analysis": {
                        "tokenizer": {
                            "edge_ngram_tokenizer": {
                                "type": "edge_ngram",
                                "min_gram": 1,
                                "max_gram": 25,
                                "token_chars": [
                                    "letter",
                                    "digit"
                                ]
                            }
                        },
                        "analyzer": {
                            "edge_ngram_analyzer": {
                                "type": "custom",
                                "tokenizer": "edge_ngram_tokenizer"
                            }
                        }
                    }
                },
                "mappings" : {
                    "properties" : {
                        // ID of the document / file / link
                        "item_id": { "type": "keyword" },
                        // Folder, File, Link
                        "item_type": { "type": "keyword" },
                        // Mime type for files
                        "mime": { "type": "keyword" },
                        // Full text file/folder/link name search
                        "name" : { "type" : "text", "analyzer": "edge_ngram_analyzer" },
                        // Full text file/link value content search
                        "content" : { "type" : "text" },
                        // Created at date search
                        "created_at": { "type": "date", "format": "rfc3339_lenient" },
                        // Exact user for the creator user ID
                        "created_by": { "type": "keyword" },
                        // Exact search for the folder the item is within
                        "folder_id": { "type": "keyword" },
                         // Exact search for the document box the item is within
                        "document_box": { "type": "keyword" },
                        // Document versioning
                        "version": {
                            "type": "keyword"
                        },
                        // Document pages for files
                        "pages": {
                            "type": "nested",
                            "properties": {
                                // Full text file/link value content search
                                "content" : { "type" : "text" },
                                // Page number
                                "page": { "type": "integer" },
                            }
                        }
                    }
                }
            }))
            .send()
            .await
            .map_err(|error| {
                tracing::error!(?error, "failed to create index");
                OpenSearchSearchError::CreateIndex
            })?;

        tracing::debug!("open search response {response:?}");

        Ok(())
    }

    async fn delete_index(&self) -> Result<(), SearchError> {
        // Delete index for files
        self.client
            .indices()
            .delete(IndicesDeleteParts::Index(&[&self.search_index.0]))
            .send()
            .await
            .map_err(|error| {
                tracing::error!(?error, "failed to delete index");
                OpenSearchSearchError::DeleteIndex
            })?;

        Ok(())
    }

    async fn search_index_file(
        &self,
        scope: &DocumentBoxScopeRaw,
        file_id: docbox_database::models::file::FileId,
        query: super::models::FileSearchRequest,
    ) -> Result<FileSearchResults, SearchError> {
        let offset = query.offset;
        let query = create_opensearch_file_query(query, scope, file_id);

        tracing::debug!(%query, "searching with query");

        // Search for field in content
        let response = self
            .client
            .search(SearchParts::Index(&[&self.search_index.0]))
            .from(offset.unwrap_or(0) as i64)
            .body(query)
            .send()
            .await
            .map_err(|error| {
                tracing::error!(?error, "failed to search index file");
                OpenSearchSearchError::SearchIndex
            })?;

        let response: serde_json::Value = response.json().await.map_err(|error| {
            tracing::error!(?error, "failed to get file search response");
            OpenSearchSearchError::SearchIndex
        })?;

        tracing::debug!(%response, "search response");

        let response: SearchResponse = serde_json::from_value(response).map_err(|error| {
            tracing::error!(?error, "failed to parse file search response");
            OpenSearchSearchError::SearchIndex
        })?;

        let (total_hits, results) = response
            .hits
            .hits
            .into_iter()
            .next()
            .and_then(|item| item.inner_hits)
            .map(|inner_hits| {
                let total_hits = inner_hits.pages.hits.total.value;
                let page_matches: Vec<PageResult> = inner_hits
                    .pages
                    .hits
                    .hits
                    .into_iter()
                    .map(|value| PageResult {
                        page: value._source.page,
                        matches: value.highlight.content,
                    })
                    .collect();
                (total_hits, page_matches)
            })
            .unwrap_or_default();

        Ok(FileSearchResults {
            total_hits,
            results,
        })
    }

    async fn search_index(
        &self,
        scope: &[DocumentBoxScopeRaw],
        query: SearchRequest,
        folder_children: Option<Vec<FolderId>>,
    ) -> Result<SearchResults, SearchError> {
        let offset = query.offset;
        let query = create_opensearch_query(query, scope, folder_children);

        tracing::debug!(%query, "searching with query");

        // Search for field in content
        let response = self
            .client
            .search(SearchParts::Index(&[&self.search_index.0]))
            .from(offset.unwrap_or(0) as i64)
            .body(query)
            .send()
            .await
            .map_err(|error| {
                tracing::error!(?error, "failed to search index");
                OpenSearchSearchError::SearchIndex
            })?;

        let response: serde_json::Value = response.json().await.map_err(|error| {
            tracing::error!(?error, "failed to get search response");
            OpenSearchSearchError::SearchIndex
        })?;

        tracing::debug!(%response);

        let response: SearchResponse = serde_json::from_value(response).map_err(|error| {
            tracing::error!(?error, "failed to parse search response");
            OpenSearchSearchError::SearchIndex
        })?;
        let total_hits = response.hits.total.value;

        const NAME_MATCH_KEYS: [&str; 2] = ["name_match_exact", "name_match_wildcard"];

        let results: Vec<FlattenedItemResult> = response
            .hits
            .hits
            .into_iter()
            .map(|item| {
                let (total_hits, page_matches) = match item.inner_hits {
                    Some(inner_hits) => {
                        let total_hits = inner_hits.pages.hits.total.value;
                        let page_matches: Vec<PageResult> = inner_hits
                            .pages
                            .hits
                            .hits
                            .into_iter()
                            .map(|value| PageResult {
                                page: value._source.page,
                                matches: value.highlight.content,
                            })
                            .collect();
                        (total_hits, page_matches)
                    }
                    None => (0, vec![]),
                };

                let name_match = item.matched_queries.is_some_and(|matches| {
                    matches
                        .iter()
                        .any(|value| NAME_MATCH_KEYS.contains(&value.as_str()))
                });
                let content_match = !page_matches.is_empty();

                FlattenedItemResult {
                    item_ty: item._source.item_type,
                    item_id: item._source.item_id,
                    document_box: item._source.document_box,
                    score: SearchScore::Float(item._score),
                    page_matches,
                    total_hits,
                    name_match,
                    content_match,
                }
            })
            .collect();

        Ok(SearchResults {
            total_hits,
            results,
        })
    }

    async fn add_data(&self, data: Vec<SearchIndexData>) -> Result<(), SearchError> {
        let mapped_data: Vec<JsonBody<OsSearchIndexData>> = data
            .into_iter()
            .map(|data| {
                JsonBody::new(OsSearchIndexData {
                    ty: data.ty,
                    folder_id: data.folder_id,
                    document_box: data.document_box,
                    item_id: data.item_id,
                    name: data.name,
                    mime: data.mime,
                    content: data.content,
                    created_at: data.created_at.to_rfc3339(),
                    created_by: data.created_by,
                    pages: data.pages,
                })
            })
            .collect();

        // Index a file
        let result = self
            .client
            // Use file.id
            .bulk(opensearch::BulkParts::Index(&self.search_index.0))
            .body(mapped_data)
            .send()
            .await
            .map_err(|error| {
                tracing::error!(?error, "failed to bulk add data");
                OpenSearchSearchError::AddData
            })?;

        let status_code = result.status_code();

        let response = result.text().await.map_err(|error| {
            tracing::error!(?error, "failed to get bulk add response");
            OpenSearchSearchError::AddData
        })?;

        if status_code.is_client_error() || status_code.is_server_error() {
            tracing::error!(?response, "bulk add error response");
            return Err(OpenSearchSearchError::AddData.into());
        }
        Ok(())
    }

    async fn update_data(
        &self,
        item_id: Uuid,
        data: UpdateSearchIndexData,
    ) -> Result<(), SearchError> {
        let data = OsUpdateSearchIndexData {
            folder_id: data.folder_id,
            name: data.name,
            content: data.content,
            pages: data.pages,
        };

        let items = self.get_by_item_id(item_id).await.map_err(|error| {
            tracing::error!(?error, "failed to find items to update");
            OpenSearchSearchError::UpdateData
        })?;

        // Nothing to update
        if items.is_empty() {
            return Ok(());
        }

        /// Structure for creating bulk update "update" or "doc" entries for serialization
        #[derive(Serialize)]
        enum BulkUpdateEntry<'a> {
            /// Update query
            #[serde(rename = "update")]
            Update {
                /// ID of the document to update
                _id: String,
            },
            /// Data to update the document with
            #[serde(rename = "doc")]
            Document {
                #[serde(flatten)]
                data: &'a OsUpdateSearchIndexData,
            },
        }

        // Create the updates
        let updates: Vec<JsonBody<BulkUpdateEntry<'_>>> = items
            .into_iter()
            .flat_map(|_id| {
                [
                    BulkUpdateEntry::Update { _id },
                    BulkUpdateEntry::Document { data: &data },
                ]
            })
            .map(JsonBody::new)
            .collect();

        // Perform the bulk updates
        let result = self
            .client
            .bulk(opensearch::BulkParts::Index(&self.search_index.0))
            .body(updates)
            .send()
            .await
            .map_err(|error| {
                tracing::error!(?error, "failed to update data (request)");
                OpenSearchSearchError::UpdateData
            })?;

        let status_code = result.status_code();
        let response: serde_json::Value = result.json().await.map_err(|error| {
            tracing::error!(?error, "failed to update data (response)");
            OpenSearchSearchError::UpdateData
        })?;

        tracing::debug!(?response, "search index update response");

        if status_code.is_client_error() || status_code.is_server_error() {
            tracing::error!(?response, "update data error response");
            return Err(OpenSearchSearchError::UpdateData.into());
        }

        Ok(())
    }

    async fn delete_data(&self, item_id: Uuid) -> Result<(), SearchError> {
        self.client
            .delete_by_query(DeleteByQueryParts::Index(&[&self.search_index.0]))
            .body(json!({
                "query": {
                    "term": { "item_id": item_id }
                }
            }))
            .send()
            .await
            .map_err(|error| {
                tracing::error!(?error, "failed to delete data");
                OpenSearchSearchError::DeleteData
            })?;

        Ok(())
    }

    async fn delete_by_scope(&self, scope: DocumentBoxScopeRaw) -> Result<(), SearchError> {
        self.client
            .delete_by_query(DeleteByQueryParts::Index(&[&self.search_index.0]))
            .body(json!({
                "query": {
                    "term": { "document_box": scope }
                }
            }))
            .send()
            .await
            .map_err(|error| {
                tracing::error!(?error, "failed to delete data by scope");
                OpenSearchSearchError::DeleteData
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

impl OpenSearchIndex {
    /// Collect all records for the provided `item_id`
    async fn get_by_item_id(&self, item_id: Uuid) -> Result<Vec<String>, OpenSearchSearchError> {
        #[derive(Debug, Deserialize, Serialize)]
        struct Response {
            hits: Hits,
        }

        #[derive(Debug, Deserialize, Serialize)]
        struct Hits {
            hits: Vec<Hit>,
        }

        #[derive(Debug, Deserialize, Serialize)]
        struct Hit {
            _id: String,
        }

        // Search for field in content
        let response = self
            .client
            .search(SearchParts::Index(&[&self.search_index.0]))
            .from(0)
            .size(10)
            .body(json!({
                "query": {
                   "term": { "item_id": item_id }
                },
            }))
            .send()
            .await
            .map_err(|error| {
                tracing::error!(?error, "failed to get search item by id");
                OpenSearchSearchError::SearchIndex
            })?;

        let response: Response = response.json().await.map_err(|error| {
            tracing::error!(?error, "failed to parse search item by id response");
            OpenSearchSearchError::SearchIndex
        })?;

        Ok(response.hits.hits.into_iter().map(|hit| hit._id).collect())
    }
}

/// Updates data within the search index
#[skip_serializing_none]
#[derive(Serialize)]
struct DateRange {
    gte: Option<String>,
    lte: Option<String>,
}

pub fn create_opensearch_query(
    req: SearchRequest,
    scopes: &[DocumentBoxScopeRaw],
    folder_children: Option<Vec<FolderId>>,
) -> serde_json::Value {
    let mut filters = vec![];
    let mut should = Vec::new();

    // Always filter to the specific document box scope
    filters.push(json!({
        "terms": { "document_box": scopes }
    }));

    let query = req
        .query
        // Filter out empty queries
        .filter(|value| !value.is_empty());

    if let Some(ref query) = query {
        if req.include_name {
            // Match name of documents
            should.push(json!({
                "term": {
                    "name": {
                        "value": query,
                        "boost": 2,
                        "_name": "name_match_exact",
                        "case_insensitive": true
                    }
                }
            }));
            should.push(json!({
                "wildcard": {
                    "name": {
                        "value": format!("*{query}*"),
                        "boost": 1.5,
                        "_name": "name_match_wildcard",
                        "case_insensitive": true
                    }
                }
            }));
        }

        if req.include_content {
            // Match content on the document itself (Link value)
            should.push(json!({
                "match": {
                    "content": {
                        "query": query,
                        // Name the match for scoring later
                        "_name": "content_match"
                    },
                }
            }));

            // Match content pages
            should.push(json!({
                "nested": {
                    "path": "pages",
                    // Match nested page content
                    "query": {
                        "match": {
                            "pages.content": {
                                "query": query,
                                // Name the match for scoring later
                                "_name": "content_match"
                            },
                        }
                    },
                    "inner_hits": {
                        "_source": ["pages.page"],
                        // Highlight
                        "highlight": {
                            "fields": {
                                "pages.content": {
                                    "fragment_size": 150,
                                    "number_of_fragments": 3,
                                    "type": "unified"
                                }
                            }
                        },
                        // Order results by score
                        "sort": [
                            {
                              "_score": {
                                "order": "desc"
                              }
                            }
                        ],
                        // Pagination
                        "size": req.max_pages.unwrap_or(3),
                    }
                }
            }));
        }
    }

    if let Some(folder_children) = folder_children {
        filters.push(json!({
            "terms": { "folder_id": folder_children }
        }));
    }

    if let Some(ref mime) = req.mime {
        filters.push(json!({
            "term": { "mime": mime }
        }));
    }

    if let Some(ref created_at) = req.created_at {
        let start = created_at.start.map(|value| value.to_rfc3339());
        let end = created_at.end.map(|value| value.to_rfc3339());

        if start.is_some() || end.is_some() {
            filters.push(json!({
                "range": {
                    "created_at": DateRange {
                        gte: start,
                        lte: end
                    }
                }
            }));
        }
    }

    if let Some(ref created_by) = req.created_by {
        filters.push(json!({
            "term": { "created_by": created_by }
        }));
    }

    // When a "should" is provided we must at least match one part of it
    let minimum_should_match = if !should.is_empty() { 1 } else { 0 };

    json!({
        // Search query itself
        "query": {
            "bool": {
                "filter": filters,
                "should": should,
                "minimum_should_match": minimum_should_match
            },
        },

        // Maximum number of results to find
        "size": req.size.unwrap_or(50),
        // Offset within results
        "from": req.offset.unwrap_or(0),

        // Only include relevant source fields
        "_source": [
            "item_id",
            "item_type",
            "document_box"
        ],

        // Sort results by match score
        "sort": [
            {
                "_score": {
                    "order": "desc"
                }
            }
        ]
    })
}

pub fn create_opensearch_file_query(
    req: FileSearchRequest,
    scope: &DocumentBoxScopeRaw,
    file_id: FileId,
) -> serde_json::Value {
    let query = req.query.unwrap_or_default();

    json!({
        // Search query itself
        "query": {
            "bool": {
                "filter": [
                    {
                        "term": { "document_box": scope }
                    },
                    {
                        "term": { "item_id": file_id }
                    }
                ],
                "should": [
                    {
                        "nested": {
                            "path": "pages",
                            // Match nested page content
                            "query": {
                                "match": {
                                    "pages.content": {
                                        "query": query,
                                        // Name the match for scoring later
                                        "_name": "content_match"
                                    },
                                }
                            },
                            "inner_hits": {
                                "_source": ["pages.page"],
                                // Highlight
                                "highlight": {
                                    "fields": {
                                        "pages.content": {
                                            "fragment_size": 150,
                                            "number_of_fragments": 1,
                                            "type": "unified"
                                        }
                                    }
                                },
                                // Order results by score
                                "sort": [
                                    {
                                    "_score": {
                                        "order": "desc"
                                    }
                                    }
                                ],
                                // Pagination
                                "size": req.limit.unwrap_or(3),
                                "from": req.offset.unwrap_or(0),
                            }
                        }
                    }
                ],
                "minimum_should_match": 1
            },
        },

        "size": 1,
        "from": 0,

        // Only include relevant source fields
        "_source": [
            "item_id",
            "item_type",
            "document_box"
        ],

        // Sort results by match score
        "sort": [
            {
                "_score": {
                    "order": "desc"
                }
            }
        ]
    })
}
