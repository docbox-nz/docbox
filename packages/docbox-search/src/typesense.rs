use super::{
    models::{
        DocumentPage, FileSearchResults, FlattenedItemResult, PageResult, SearchIndexType,
        SearchRequest, SearchResults, SearchScore, UpdateSearchIndexData,
    },
    SearchIndex,
};
use anyhow::Context;
use docbox_database::models::{document_box::DocumentBoxScopeRaw, folder::FolderId, user::UserId};
use itertools::Itertools;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_with::skip_serializing_none;
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize)]
pub struct TypesenseSearchConfig {
    pub url: String,
    pub api_key: String,
}

impl TypesenseSearchConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let url = std::env::var("TYPESENSE_URL").context("missing TYPESENSE_URL env")?;
        let api_key =
            std::env::var("TYPESENSE_API_KEY").context("missing TYPESENSE_API_KEY env")?;
        Ok(Self { url, api_key })
    }
}

#[derive(Clone)]
pub struct TypesenseIndexFactory {
    client: reqwest::Client,
    base_url: String,
}

impl TypesenseIndexFactory {
    pub fn from_config(config: TypesenseSearchConfig) -> anyhow::Result<Self> {
        Self::new(config.url, config.api_key)
    }

    pub fn new(base_url: String, api_key: String) -> anyhow::Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("x-typesense-api-key"),
            HeaderValue::from_str(&api_key)?,
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            // Don't try and proxy through the proxy
            .no_proxy()
            .build()?;

        Ok(Self { client, base_url })
    }

    pub fn create_search_index(&self, index: String) -> TypesenseIndex {
        TypesenseIndex {
            client: self.client.clone(),
            base_url: self.base_url.clone(),
            index,
        }
    }
}

pub struct TypesenseIndex {
    client: reqwest::Client,
    base_url: String,
    index: String,
}

/// Document entry within the typesense index
#[derive(Serialize, Deserialize)]
struct TypesenseEntry {
    /// Unique ID for the entry
    id: Uuid,

    /// Entry data
    #[serde(flatten)]
    entry: TypesenseDataEntry,
}

/// Wrapper around the entry to support versioning for
/// future changes in structure
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "version")]
enum TypesenseDataEntry {
    /// Current V1 entry format
    V1(TypesenseDataEntryV1),
}

/// Type of document entries
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "entry_type")]
enum TypesenseDataEntryV1 {
    /// Root entry, all items will have one of these. This contains the base
    /// information for the entry
    Root(TypesenseDataEntryRootV1),
    /// Page entry, document present for each page of text content indexed files
    /// for full text content search
    Page(TypesenseDataEntryPageV1),
}

/// Root entry data for the item itself
#[derive(Debug, Clone, Serialize, Deserialize)]
#[skip_serializing_none]
struct TypesenseDataEntryRootV1 {
    /// Scope the entry is within
    document_box: DocumentBoxScopeRaw,
    /// ID of the folder the entry is within
    folder_id: FolderId,

    /// Type of entry
    #[serde(rename = "item_type")]
    ty: SearchIndexType,
    /// ID of the (Folder/File/Link) itself
    item_id: Uuid,
    /// Name of the item
    name: String,

    /// URL value if the item is a link
    value: Option<String>,
    /// Mime value if the item is a file
    mime: Option<String>,

    /// Creation date for the item (Unix timestamp)
    created_at: i64,
    /// User who created the item
    created_by: Option<UserId>,
}

/// Page entry for an item page
#[derive(Debug, Serialize, Deserialize)]
struct TypesenseDataEntryPageV1 {
    /// Every page includes a copy of the root
    #[serde(flatten)]
    root: TypesenseDataEntryRootV1,

    /// Page number
    page: u64,

    /// Content contained within the page
    /// (Ignored when loading results back)
    page_content: Option<String>,
}

#[derive(Deserialize)]
struct SearchResponse<T> {
    results: Vec<T>,
}

#[derive(Deserialize)]
#[allow(unused)]
struct GenericSearchResponse {
    found: u64,
    hits: Vec<Hit>,
}

#[derive(Deserialize)]
struct GroupedSearchResponse {
    found: u64,
    grouped_hits: Vec<GroupedHits>,
}

#[derive(Deserialize)]
struct GroupedHits {
    found: u64,
    hits: Vec<Hit>,
}

#[derive(Deserialize)]
struct Hit {
    document: TypesenseDataEntry,
    highlights: Vec<Highlight>,
    text_match: u64,
}

#[derive(Deserialize)]
struct Highlight {
    field: String,
    snippet: String,
}

fn escape_typesense_value(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace('{', "\\{")
        .replace('}', "\\}")
        .replace(':', "\\:")
        .replace(',', "\\,")
        .replace('\'', "\\'")
        .replace('"', "\\\"")
        .replace('<', "\\<")
        .replace('>', "\\>")
        .replace('=', "\\=")
}

impl SearchIndex for TypesenseIndex {
    async fn create_index(&self) -> anyhow::Result<()> {
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
            .post(format!("{}/collections", self.base_url))
            .json(&schema)
            .send()
            .await?
            .error_for_status()?;

        Ok(())
    }

    async fn delete_index(&self) -> anyhow::Result<()> {
        self.client
            .delete(format!("{}/collections/{}", self.base_url, self.index))
            .send()
            .await?
            .error_for_status()?;

        Ok(())
    }

    async fn search_index_file(
        &self,
        scope: &DocumentBoxScopeRaw,
        file_id: docbox_database::models::file::FileId,
        query: super::models::FileSearchRequest,
    ) -> anyhow::Result<FileSearchResults> {
        let offset = query.offset.unwrap_or(0);
        let limit = query.limit.unwrap_or(50);
        let query = query.query.unwrap_or_default();
        let filter_by = format!(
            r#"document_box:="{}"&&item_id:="{}"&&entry_type:="Page""#,
            escape_typesense_value(scope),
            escape_typesense_value(&file_id.to_string())
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
            .post(format!("{}/multi_search", self.base_url,))
            .json(&query_json)
            .send()
            .await
            .inspect_err(|error| {
                tracing::error!(?error, "failed to query typesense multi_search")
            })?;

        if let Err(error) = response.error_for_status_ref() {
            let body = response.text().await;
            tracing::error!(?error, ?body, "failed to get search results");
            return Err(error.into());
        }

        let search: SearchResponse<GenericSearchResponse> = response.json().await?;
        let search = search
            .results
            .into_iter()
            .next()
            .context("missing search result")?;

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
        scopes: &[docbox_database::models::document_box::DocumentBoxScopeRaw],
        query: super::models::SearchRequest,
        folder_children: Option<Vec<docbox_database::models::folder::FolderId>>,
    ) -> anyhow::Result<super::models::SearchResults> {
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

        // Must query at least one field
        if query_by.is_empty() {
            return Err(anyhow::anyhow!(
                "must provide either include_name or include_content"
            ));
        }

        let filter_by = Self::create_search_filters(scopes, &query, folder_children);

        let size = query.size.unwrap_or(50);
        let offset = query.offset.unwrap_or(0);

        let max_pages = query.max_pages.unwrap_or(3);
        let query = query.query.unwrap_or_default();

        let query_by = query_by.join(",");

        let query_json = json!({
            "searches": [
                {
                    "collection": self.index,
                    "q": query,
                    "query_by": query_by,
                    "group_by": "item_id",
                    "group_limit": max_pages,
                    "offset": offset,
                    "limit": size,
                    "filter_by": filter_by,
                    "exclude_fields": "page_content",
                    "highlight_fields": "name,value,page_content",
                    "highlight_start_tag": "<em>",
                    "highlight_end_tag": "</em>"
                }
            ]
        });

        tracing::debug!(?query_json);
        let response = self
            .client
            .post(format!("{}/multi_search", self.base_url,))
            .json(&query_json)
            .send()
            .await
            .inspect_err(|error| {
                tracing::error!(?error, "failed to query typesense multi_search")
            })?;

        if let Err(error) = response.error_for_status_ref() {
            let body = response.text().await;
            let query = serde_json::to_string(&query_json);
            tracing::error!(?error, ?body, ?query, "failed to get search results");
            return Err(error.into());
        }

        let response: serde_json::Value = response.json().await?;
        tracing::debug!(?response);

        let search: SearchResponse<GroupedSearchResponse> = serde_json::from_value(response)?;
        let search = search
            .results
            .into_iter()
            .next()
            .context("missing search result")?;

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

    async fn bulk_add_data(&self, data: Vec<super::models::SearchIndexData>) -> anyhow::Result<()> {
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
                    documents.push(Self::create_item_page(&root, page));
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

    async fn add_data(&self, data: super::models::SearchIndexData) -> anyhow::Result<()> {
        let mut documents = Vec::new();
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

        if let Some(pages) = data.pages {
            for page in pages {
                documents.push(Self::create_item_page(&root, page));
            }
        }

        documents.push(TypesenseEntry {
            id: Uuid::new_v4(),
            entry: TypesenseDataEntry::V1(TypesenseDataEntryV1::Root(root)),
        });

        self.bulk_add_documents(documents).await?;

        Ok(())
    }

    async fn update_data(
        &self,
        item_id: uuid::Uuid,
        data: super::models::UpdateSearchIndexData,
    ) -> anyhow::Result<()> {
        // Update all the existing items so they have the current root data
        self.update_item_roots(item_id, &data).await?;

        if let Some(pages) = data.pages {
            // Delete all page based documents
            self.delete_item_pages(item_id).await?;

            // Resolve the root document
            let root = self
                .get_item_root(item_id)
                .await?
                .context("missing root entry to update")?;

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
                .map(|page| Self::create_item_page(&updated_root, page))
                .collect();

            // Bulk insert the created documents
            self.bulk_add_documents(documents).await?;
        }

        Ok(())
    }

    async fn delete_data(&self, id: uuid::Uuid) -> anyhow::Result<()> {
        self.client
            .delete(format!(
                "{}/collections/{}/documents",
                self.base_url, self.index
            ))
            .query(&[("filter_by", format!(r#"item_id:="{id}""#))])
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    async fn delete_by_scope(
        &self,
        scope: docbox_database::models::document_box::DocumentBoxScopeRaw,
    ) -> anyhow::Result<()> {
        self.client
            .delete(format!(
                "{}/collections/{}/documents",
                self.base_url, self.index
            ))
            .query(&[("filter_by", format!(r#"document_box:="{scope}""#))])
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    async fn apply_migration(&self, _name: &str) -> anyhow::Result<()> {
        Ok(())
    }
}

impl TypesenseIndex {
    /// Bulk insert typesense documents
    async fn bulk_add_documents(&self, entries: Vec<TypesenseEntry>) -> anyhow::Result<()> {
        // Encode entries into newline delimitated encoded JSON strings
        let mut bulk_data = String::new();
        for document in entries {
            let value = serde_json::to_string(&document)?;
            bulk_data.push_str(&value);
            bulk_data.push('\n');
        }

        self.client
            .post(format!(
                "{}/collections/{}/documents/import",
                self.base_url, self.index
            ))
            .body(bulk_data)
            .send()
            .await
            .inspect_err(|error| tracing::error!(?error, "failed to send import documents"))?
            .error_for_status()
            .inspect_err(|error| {
                tracing::error!(?error, "error status when importing documents")
            })?;

        Ok(())
    }

    /// Deletes all "Page" item types for a specific `item_id`
    async fn delete_item_pages(&self, item_id: Uuid) -> anyhow::Result<()> {
        self.client
            .delete(format!(
                "{}/collections/{}/documents",
                self.base_url, self.index
            ))
            .query(&[(
                "filter_by",
                format!(r#"item_id:="{item_id}"&&entry_type="Page""#),
            )])
            .send()
            .await
            .inspect_err(|error| tracing::error!(?error, "failed to send delete documents"))?
            .error_for_status()
            .inspect_err(|error| tracing::error!(?error, "error status when deleting documents"))?;

        Ok(())
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

    /// Updates the root portion of all documents for the provided item
    async fn update_item_roots(
        &self,
        item_id: Uuid,
        update: &UpdateSearchIndexData,
    ) -> anyhow::Result<()> {
        let request = json!({
            "folder_id": update.folder_id,
            "name": update.name,
            "value": update.content,
        });

        // Update all the existing items so they have the current root data
        self.client
            .patch(format!(
                "{}/collections/{}/documents",
                self.base_url, self.index
            ))
            .query(&[("filter_by", format!(r#"item_id:="{item_id}""#))])
            .json(&request)
            .send()
            .await
            .inspect_err(|error| tracing::error!(?error, "failed to send update documents"))?
            .error_for_status()
            .inspect_err(|error| tracing::error!(?error, "error status when updating documents"))?;

        Ok(())
    }

    /// Get the "root" item document for the provided `item_id`
    async fn get_item_root(
        &self,
        item_id: Uuid,
    ) -> anyhow::Result<Option<TypesenseDataEntryRootV1>> {
        let response: GenericSearchResponse = self
            .client
            .get(format!(
                "{}/collections/{}/documents/search",
                self.base_url, self.index
            ))
            .query(&[(
                "filter_by",
                format!(r#"item_id:="{item_id}"&&entry_type=Root"#),
            )])
            .send()
            .await
            .inspect_err(|error| tracing::error!(?error, "failed to query documents search"))?
            .error_for_status()?
            .json()
            .await
            .inspect_err(|error| tracing::error!(?error, "error when deserializing response"))?;

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
                .map(|value| format!("\"{}\"", escape_typesense_value(value)))
                .join(", ");

            filter_parts.push(format!("document_box:=[{scopes}]"));
        }

        // Filter to children of allowed folders
        if let Some(folder_children) = folder_children {
            if !folder_children.is_empty() {
                let ids = folder_children
                    .into_iter()
                    .map(|value| format!("\"{}\"", escape_typesense_value(&value.to_string())))
                    .join(", ");

                filter_parts.push(format!("folder_id:=[{ids}]"));
            }
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
                escape_typesense_value(created_by)
            ));
        }

        if let Some(folder_id) = query.folder_id {
            filter_parts.push(format!(
                r#"folder_id:="{}""#,
                escape_typesense_value(&folder_id.to_string())
            ));
        }

        filter_parts.join("&&")
    }
}
