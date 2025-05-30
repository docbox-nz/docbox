use anyhow::Context;
use docbox_database::models::{document_box::DocumentBoxScope, folder::FolderId, user::UserId};
use itertools::Itertools;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_with::skip_serializing_none;
use uuid::Uuid;

use crate::search::models::{FlattenedItemResult, PageResult};

use super::{
    models::{FileSearchResults, SearchIndexType, SearchResults},
    SearchIndex,
};

#[derive(Clone)]
pub struct TypesenseIndexFactory {
    client: reqwest::Client,
    base_url: String,
}

impl TypesenseIndexFactory {
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

#[derive(Serialize, Deserialize)]
pub struct TypesenseEntry {
    pub id: Uuid,
    #[serde(flatten)]
    pub entry: TypesenseDataEntry,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "version")]
pub enum TypesenseDataEntry {
    V1(TypesenseDataEntryV1),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "entry_type")]
pub enum TypesenseDataEntryV1 {
    Root(TypesenseDataEntryRootV1),
    Page(TypesenseDataEntryPageV1),
}

/// Root entry data for the item itself
#[derive(Debug, Clone, Serialize, Deserialize)]
#[skip_serializing_none]
pub struct TypesenseDataEntryRootV1 {
    /// Scope the entry is within
    pub document_box: DocumentBoxScope,
    /// ID of the folder the entry is within
    pub folder_id: FolderId,

    /// Type of entry
    #[serde(rename = "item_type")]
    pub ty: SearchIndexType,
    /// ID of the (Folder/File/Link) itself
    pub item_id: Uuid,
    /// Name of the item
    pub name: String,

    /// URL value if the item is a link
    pub value: Option<String>,
    /// Mime value if the item is a file
    pub mime: Option<String>,

    /// Creation date for the item (Unix timestamp)
    pub created_at: i64,
    /// User who created the item
    pub created_by: Option<UserId>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TypesenseDataEntryPageV1 {
    /// Every page includes a copy of the root
    #[serde(flatten)]
    pub root: TypesenseDataEntryRootV1,

    /// Page number
    pub page: u64,

    /// Content contained within the page
    /// (Ignored when loading results back)
    pub page_content: Option<String>,
}

#[derive(Deserialize)]
#[allow(unused)]
struct MultiSearchResponse {
    results: Vec<GroupedSearchResponse>,
}

#[derive(Deserialize)]
#[allow(unused)]
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
#[allow(unused)]
struct GroupedSearchResponse {
    found: u64,
    found_docs: u64,
    grouped_hits: Vec<GroupedHits>,
}

#[derive(Deserialize)]
#[allow(unused)]
struct GroupedHits {
    found: u64,
    group_key: Vec<String>,
    hits: Vec<Hit>,
}

#[derive(Deserialize)]
struct Hit {
    document: TypesenseDataEntry,
    highlights: Vec<Highlight>,
    text_match: u64,
}

#[derive(Deserialize)]
#[allow(unused)]
struct Highlight {
    field: String,
    matched_tokens: Vec<String>,
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
        scope: &DocumentBoxScope,
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
                    "q": query.as_str(),
                    "query_by": "page_content",
                    "offset": offset.to_string().as_str(),
                    "limit": limit.to_string().as_str(),
                    "filter_by": filter_by.as_str(),
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
            .await?;

        if let Err(error) = response.error_for_status_ref() {
            let body = response.text().await;
            tracing::error!(?error, ?body, "failed to get search results");
            return Err(error.into());
        }

        let search: SearchResponse<GenericSearchResponse> = response.json().await?;
        let search = search.results.first().context("missing search result")?;

        let total_hits = search.found;
        let results: Vec<PageResult> = search
            .hits
            .iter()
            .filter_map(|hit| {
                let entry = match &hit.document {
                    TypesenseDataEntry::V1(TypesenseDataEntryV1::Page(page)) => page,
                    _ => return None,
                };

                let highlighted = hit
                    .highlights
                    .iter()
                    .find(|value| value.field == "page_content")
                    .map(|value| value.snippet.to_string())?;

                Some(PageResult {
                    page: entry.page,
                    matches: vec![highlighted],
                })
            })
            .collect();

        Ok(FileSearchResults {
            total_hits,
            results,
        })
    }

    async fn search_index(
        &self,
        scopes: &[docbox_database::models::document_box::DocumentBoxScope],
        query: super::models::SearchRequest,
        folder_children: Option<Vec<docbox_database::models::folder::FolderId>>,
    ) -> anyhow::Result<super::models::SearchResults> {
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

        if let Some(range) = query.created_at {
            if let Some(start) = range.start {
                let start = start.timestamp();
                filter_parts.push(format!(r#"created_at:>"{start}""#));
            }

            if let Some(end) = range.end {
                let end = end.timestamp();
                filter_parts.push(format!(r#"created_at:<"{end}""#));
            }
        }

        // if let Some(range) = query.modified {
        //     // TODO: ...modified date query
        // }

        if let Some(created_by) = query.created_by {
            filter_parts.push(format!(
                r#"created_by:="{}""#,
                escape_typesense_value(&created_by)
            ));
        }

        if let Some(folder_id) = query.folder_id {
            filter_parts.push(format!(
                r#"folder_id:="{}""#,
                escape_typesense_value(&folder_id.to_string())
            ));
        }

        if let Some(item_id) = query.item_id {
            filter_parts.push(format!(
                r#"item_id:="{}""#,
                escape_typesense_value(&item_id.to_string())
            ));
        }

        let mut query_by = Vec::new();
        if query.include_name {
            query_by.push("name");
        }

        if query.include_content {
            query_by.push("value");
            query_by.push("page_content");
        }

        if query_by.is_empty() {
            return Err(anyhow::anyhow!(
                "must provide either include_name or include_content"
            ));
        }

        let size = query.size.unwrap_or(50);
        let offset = query.offset.unwrap_or(0);

        let max_pages = query.max_pages.unwrap_or(3);
        let _pages_offset = query.pages_offset.unwrap_or(0);

        // TODO: When searching for a specific item_id, disable group_by and use
        // pages_offset instead. to emulate previous opensearch functionality

        let query = query.query.unwrap_or_default();

        let filter_by = filter_parts.join("&&");
        let query_by = query_by.join(",");

        let query_json = json!({
            "searches": [
                {
                    "collection": self.index,
                    "q": query.as_str(),
                    "query_by": query_by.as_str(),
                    "group_by": "item_id",
                    "group_limit": max_pages.to_string().as_str(),
                    "offset": offset.to_string().as_str(),
                    "limit": size.to_string().as_str(),
                    "filter_by": filter_by.as_str(),
                    "exclude_fields": "page_content",
                    "highlight_fields": "name,value,page_content",
                    "highlight_start_tag": "<em>",
                    "highlight_end_tag": "</em>"
                }
            ]
        });

        tracing::debug!(query_json = %serde_json::to_string(&query_json).unwrap(), "search query");

        let response = self
            .client
            .post(format!("{}/multi_search", self.base_url,))
            .json(&query_json)
            .send()
            .await?;

        if let Err(error) = response.error_for_status_ref() {
            let body = response.text().await;
            tracing::error!(?error, ?body, "failed to get search results");
            return Err(error.into());
        }

        let search: SearchResponse<GroupedSearchResponse> = response.json().await?;
        let search = search.results.first().context("missing search result")?;

        let total_hits = search.found;

        let mut results: Vec<FlattenedItemResult> = Vec::new();

        for group in &search.grouped_hits {
            let root = match group.hits.first() {
                Some(value) => &value.document,
                None => continue,
            };

            match root {
                TypesenseDataEntry::V1(TypesenseDataEntryV1::Root(root))
                | TypesenseDataEntry::V1(TypesenseDataEntryV1::Page(TypesenseDataEntryPageV1 {
                    root,
                    ..
                })) => {
                    let group_score = group
                        .hits
                        .iter()
                        .map(|hit| hit.text_match)
                        .max()
                        .unwrap_or_default();

                    let name_match = group.hits.iter().any(|hit| {
                        hit.highlights
                            .iter()
                            .any(|highlight| highlight.field == "name")
                    });

                    let content_match = group.hits.iter().any(|hit| {
                        hit.highlights.iter().any(|highlight| {
                            highlight.field == "value" || highlight.field == "page_content"
                        })
                    });

                    let page_matches: Vec<PageResult> = group
                        .hits
                        .iter()
                        .filter_map(|hit| {
                            let entry = match &hit.document {
                                TypesenseDataEntry::V1(TypesenseDataEntryV1::Page(page)) => page,
                                _ => return None,
                            };

                            let highlighted = hit
                                .highlights
                                .iter()
                                .find(|value| value.field == "page_content")
                                .map(|value| value.snippet.to_string())?;

                            Some(PageResult {
                                page: entry.page,
                                matches: vec![highlighted],
                            })
                        })
                        .collect();

                    results.push(FlattenedItemResult {
                        item_ty: root.ty,
                        item_id: root.item_id,
                        document_box: root.document_box.clone(),
                        page_matches,
                        total_hits: group.found,
                        score: group_score as f32,
                        name_match,
                        content_match,
                    });
                }
            }
        }

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
                    documents.push(TypesenseEntry {
                        id: Uuid::new_v4(),
                        entry: TypesenseDataEntry::V1(TypesenseDataEntryV1::Page(
                            TypesenseDataEntryPageV1 {
                                root: root.clone(),
                                page: page.page,
                                page_content: Some(page.content),
                            },
                        )),
                    });
                }
            }

            // Create the root document
            documents.push(TypesenseEntry {
                id: Uuid::new_v4(),
                entry: TypesenseDataEntry::V1(TypesenseDataEntryV1::Root(root)),
            });
        }

        let mut bulk_data = String::new();
        for document in documents {
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
            .await?
            .error_for_status()?;

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

        // When its a file with page data
        if let Some(pages) = data.pages {
            // Add a new entry for each page
            for page in pages {
                documents.push(TypesenseEntry {
                    id: Uuid::new_v4(),
                    entry: TypesenseDataEntry::V1(TypesenseDataEntryV1::Page(
                        TypesenseDataEntryPageV1 {
                            root: root.clone(),
                            page: page.page,
                            page_content: Some(page.content),
                        },
                    )),
                });
            }
        }

        // Create the root document
        documents.push(TypesenseEntry {
            id: Uuid::new_v4(),
            entry: TypesenseDataEntry::V1(TypesenseDataEntryV1::Root(root)),
        });

        let mut bulk_data = String::new();
        for document in documents {
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
            .await?
            .error_for_status()?;

        Ok(())
    }

    async fn update_data(
        &self,
        item_id: uuid::Uuid,
        data: super::models::UpdateSearchIndexData,
    ) -> anyhow::Result<()> {
        // Update all the existing items so they have the current root data
        self.client
            .patch(format!(
                "{}/collections/{}/documents",
                self.base_url, self.index
            ))
            .query(&[("filter_by", format!(r#"item_id:="{item_id}""#))])
            .json(&json!({
                "folder_id":  data.folder_id,
                "name": data.name.clone(),
                "value": data.content.clone(),
            }))
            .send()
            .await?
            .error_for_status()?;

        #[derive(Deserialize)]
        struct Response {
            hits: Vec<Hit>,
        }

        #[derive(Deserialize)]
        struct Hit {
            document: TypesenseDataEntry,
        }

        let response: Response = self
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
            .await?
            .error_for_status()?
            .json()
            .await?;

        // Resolve the root document
        let root = response
            .hits
            .first()
            .and_then(|hit| match &hit.document {
                TypesenseDataEntry::V1(TypesenseDataEntryV1::Root(root)) => Some(root),
                _ => None,
            })
            .context("missing root entry to update")?;

        let mut added_documents = Vec::new();

        if let Some(pages) = data.pages {
            // Delete all page based documents
            {
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
                    .await?
                    .error_for_status()?;
            }

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

            // Create new documents
            for page in pages {
                added_documents.push(TypesenseEntry {
                    id: Uuid::new_v4(),
                    entry: TypesenseDataEntry::V1(TypesenseDataEntryV1::Page(
                        TypesenseDataEntryPageV1 {
                            root: updated_root.clone(),
                            page: page.page,
                            page_content: Some(page.content),
                        },
                    )),
                });
            }
        }

        // Add the new page data
        if !added_documents.is_empty() {
            let mut bulk_data = String::new();
            for document in added_documents {
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
                .await?
                .error_for_status()?;
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
        scope: docbox_database::models::document_box::DocumentBoxScope,
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
}
