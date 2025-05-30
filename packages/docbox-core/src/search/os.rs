use crate::search::{
    models::{FlattenedItemResult, PageResult},
    os_models,
};
use anyhow::Context;
use aws_config::SdkConfig;
use docbox_database::{
    models::{
        document_box::DocumentBoxScope,
        file::File,
        folder::{Folder, FolderId, FolderPathSegment},
        link::Link,
        tenant::Tenant,
        user::UserId,
    },
    DbPool,
};
use opensearch::{
    http::{
        request::JsonBody,
        transport::{SingleNodeConnectionPool, TransportBuilder},
        Url,
    },
    indices::{IndicesCreateParts, IndicesDeleteParts},
    DeleteByQueryParts, IndexParts, OpenSearch, SearchParts,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_with::skip_serializing_none;
use uuid::Uuid;

use super::{
    models::{
        DocumentPage, FileSearchResults, SearchIndexData, SearchIndexType, SearchRequest,
        SearchResultData, SearchResults, UpdateSearchIndexData,
    },
    SearchIndex,
};

#[derive(Clone)]
pub struct OpenSearchIndexFactory {
    client: OpenSearch,
}

impl OpenSearchIndexFactory {
    pub fn new(client: OpenSearch) -> Self {
        Self { client }
    }

    pub fn create_search_index(&self, search_index: TenantSearchIndexName) -> OpenSearchIndex {
        OpenSearchIndex {
            client: self.client.clone(),
            search_index,
        }
    }
}

pub struct OpenSearchIndex {
    client: OpenSearch,
    search_index: TenantSearchIndexName,
}

/// Represents a search index name for a specific tenant
#[derive(Debug)]
pub struct TenantSearchIndexName(String);

impl TenantSearchIndexName {
    pub fn from_tenant(tenant: &Tenant) -> Self {
        Self(tenant.os_index_name.clone())
    }
}

/// Create instance of [OpenSearch] from the environment
pub fn create_open_search(aws_config: &SdkConfig, url: Url) -> anyhow::Result<OpenSearch> {
    if cfg!(debug_assertions) {
        create_open_search_dev(url)
    } else {
        create_open_search_prod(aws_config, url)
    }
}

/// Create instance of [OpenSearch] from the environment
pub fn create_open_search_dev(url: Url) -> anyhow::Result<OpenSearch> {
    let conn_pool = SingleNodeConnectionPool::new(url);

    let transport = TransportBuilder::new(conn_pool)
        // We don't want open search trying to access the index through our proxy. It has a direct route
        .disable_proxy()
        // Disable certificate validation for local development
        .cert_validation(opensearch::cert::CertificateValidation::None)
        .build()
        .context("failed to build open search transport")?;

    let open_search = OpenSearch::new(transport);

    Ok(open_search)
}

/// Create instance of [OpenSearch] from the environment
pub fn create_open_search_prod(aws_config: &SdkConfig, url: Url) -> anyhow::Result<OpenSearch> {
    // Setup opensearch connection pool
    let conn_pool = SingleNodeConnectionPool::new(url);

    let transport = TransportBuilder::new(conn_pool)
        // We don't want open search trying to access the index through our proxy. It has a direct route
        .disable_proxy()
        .auth(aws_config.clone().try_into()?)
        .service_name("es")
        .build()
        .context("failed to build open search transport")?;

    let open_search = OpenSearch::new(transport);

    Ok(open_search)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OsSearchIndexData {
    /// Type of item the search index data is representing
    #[serde(rename = "item_type")]
    pub ty: SearchIndexType,

    /// ID of the folder the indexed item is within.
    ///
    /// (For searching only withing a specific folder path)
    pub folder_id: FolderId,
    /// Document box scope that this item is within
    ///
    /// (For restricting search scope)
    pub document_box: DocumentBoxScope,

    /// Unique ID for the actual document
    ///
    /// this is to allow multiple page documents to be stored as
    /// separate search index items without overriding each other
    pub item_id: Uuid,
    /// Name of this item
    pub name: String,
    /// Mime type when working with file items (Otherwise none)
    pub mime: Option<String>,
    /// For files this is the file content (With an associated page number)
    /// For links this is the link value
    pub content: Option<String>,
    /// Creation date for the item
    pub created_at: String,
    /// User who created the item
    pub created_by: Option<UserId>,
    /// Optional pages of document content
    pub pages: Option<Vec<DocumentPage>>,
}

#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize)]
pub struct OsUpdateSearchIndexData {
    pub folder_id: FolderId,
    pub name: String,
    pub content: Option<String>,
    pub pages: Option<Vec<DocumentPage>>,
}

impl SearchIndex for OpenSearchIndex {
    async fn create_index(&self) -> anyhow::Result<()> {
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
            .await?;

        tracing::debug!("open search response {response:?}");

        Ok(())
    }

    async fn delete_index(&self) -> anyhow::Result<()> {
        // Delete index for files
        self.client
            .indices()
            .delete(IndicesDeleteParts::Index(&[&self.search_index.0]))
            .send()
            .await?;

        Ok(())
    }

    async fn search_index_file(
        &self,
        _scope: &DocumentBoxScope,
        _file_id: docbox_database::models::file::FileId,
        _query: super::models::FileSearchRequest,
    ) -> anyhow::Result<FileSearchResults> {
        anyhow::bail!("search index file is not currently supported on the opensearch backend");
    }

    async fn search_index(
        &self,
        scope: &[DocumentBoxScope],
        query: SearchRequest,
        folder_children: Option<Vec<FolderId>>,
    ) -> anyhow::Result<SearchResults> {
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
            .await?;

        let response: serde_json::Value =
            response.json().await.context("failed to parse response")?;

        tracing::debug!(%response);

        let response: os_models::SearchResponse = serde_json::from_value(response)?;
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
                    score: item._score,
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

    async fn bulk_add_data(&self, data: Vec<SearchIndexData>) -> anyhow::Result<()> {
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
            .await?;

        let status_code = result.status_code();

        let response = result.text().await?;

        if status_code.is_client_error() || status_code.is_server_error() {
            return Err(anyhow::anyhow!("error response: {response}"));
        }
        Ok(())
    }

    async fn add_data(&self, data: SearchIndexData) -> anyhow::Result<()> {
        let data = OsSearchIndexData {
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
        };

        // Index a file
        let result = self
            .client
            // Use file.id
            .index(IndexParts::Index(&self.search_index.0))
            .body(&data)
            .send()
            .await?;

        let status_code = result.status_code();

        let response: serde_json::Value = result.json().await?;

        tracing::debug!(?response, "search index add response");

        if status_code.is_client_error() || status_code.is_server_error() {
            return Err(anyhow::anyhow!("error response: {response}"));
        }

        Ok(())
    }

    async fn update_data(&self, item_id: Uuid, data: UpdateSearchIndexData) -> anyhow::Result<()> {
        let data = OsUpdateSearchIndexData {
            folder_id: data.folder_id,
            name: data.name,
            content: data.content,
            pages: data.pages,
        };

        let items = self
            .get_by_item_id(item_id)
            .await
            .context("failed to find items to update")?;

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
            .await?;

        let status_code = result.status_code();
        let response: serde_json::Value = result.json().await?;

        tracing::debug!(?response, "search index update response");

        if status_code.is_client_error() || status_code.is_server_error() {
            return Err(anyhow::anyhow!("error response: {response}"));
        }

        Ok(())
    }

    async fn delete_data(&self, item_id: Uuid) -> anyhow::Result<()> {
        self.client
            .delete_by_query(DeleteByQueryParts::Index(&[&self.search_index.0]))
            .body(json!({
                "query": {
                    "term": { "item_id": item_id }
                }
            }))
            .send()
            .await?;

        Ok(())
    }

    async fn delete_by_scope(&self, scope: DocumentBoxScope) -> anyhow::Result<()> {
        self.client
            .delete_by_query(DeleteByQueryParts::Index(&[&self.search_index.0]))
            .body(json!({
                "query": {
                    "term": { "document_box": scope }
                }
            }))
            .send()
            .await?;

        Ok(())
    }
}

impl OpenSearchIndex {
    /// Collect all records for the provided `item_id`
    async fn get_by_item_id(&self, item_id: Uuid) -> anyhow::Result<Vec<String>> {
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
            .await?;

        let response: Response = response.json().await.context("failed to parse response")?;

        Ok(response.hits.hits.into_iter().map(|hit| hit._id).collect())
    }
}

pub async fn resolve_search_result(
    db: &DbPool,
    hit: FlattenedItemResult,
) -> anyhow::Result<(
    FlattenedItemResult,
    SearchResultData,
    Vec<FolderPathSegment>,
)> {
    let (data, path) = match hit.item_ty {
        SearchIndexType::File => {
            let file = File::find_with_extra(db, &hit.document_box, hit.item_id)
                .await
                .context("failed to query file")?
                .context("file present in search results doesn't exist")?;
            let path = File::resolve_path(db, hit.item_id).await?;

            (SearchResultData::File(file), path)
        }
        SearchIndexType::Folder => {
            let folder = Folder::find_by_id_with_extra(db, &hit.document_box, hit.item_id)
                .await
                .context("failed to query folder")?
                .context("folder present in search results doesn't exist")?;
            let path = Folder::resolve_path(db, hit.item_id).await?;

            (SearchResultData::Folder(folder), path)
        }
        SearchIndexType::Link => {
            let link = Link::find_with_extra(db, &hit.document_box, hit.item_id)
                .await
                .context("failed to query link")?
                .context("link present in search results doesn't exist")?;
            let path = Link::resolve_path(db, hit.item_id).await?;

            (SearchResultData::Link(link), path)
        }
    };

    Ok((hit, data, path))
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
    scopes: &[DocumentBoxScope],
    folder_children: Option<Vec<FolderId>>,
) -> serde_json::Value {
    let mut filters = vec![];
    let mut should = Vec::new();

    // Always filter to the specific document box scope
    filters.push(json!({
        "terms": { "document_box": scopes }
    }));

    // Filter results to a specific file
    if let Some(item_id) = req.item_id {
        filters.push(json!({
            "term": { "item_id": item_id }
        }));
    }

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
                                    "fragment_size": req.max_fragment_size.unwrap_or(150),
                                    "number_of_fragments": req.max_fragments.unwrap_or(3),
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
                        "from": req.pages_offset.unwrap_or(0),
                    }
                }
            }));
        }

        // if req.neural {
        //     let model_id = std::env::var("OPENSEARCH_MODEL_ID");
        //     if let Ok(model_id) = model_id {
        //         must.push(json!({
        //                 "neural": {
        //                     "content_embedding": {
        //                         "query_text": query,
        //                         "model_id": model_id,
        //                         "k": 200
        //                     }
        //                 }
        //         }))
        //     }
        // } else if !fields.is_empty() {
        //     must.push(json!({
        //             "multi_match": {
        //                 "query": query,
        //                 "type": "bool_prefix",
        //                 "fields": fields
        //             }
        //     }))
        // }
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
