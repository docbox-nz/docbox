//! # Search Models
//!
//! This file contains the models for serializing and deserializing
//! search data

use chrono::{DateTime, Utc};
use docbox_database::models::{
    document_box::{DocumentBoxScopeRaw, WithScope},
    file::FileWithExtra,
    folder::{FolderId, FolderWithExtra},
    link::LinkWithExtra,
    shared::FolderPathSegment,
    user::{User, UserId},
};
use garde::Validate;
use mime::Mime;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, skip_serializing_none};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SearchIndexType {
    File,
    Folder,
    Link,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchIndexData {
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
    pub document_box: DocumentBoxScopeRaw,

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
    pub created_at: DateTime<Utc>,
    /// User who created the item
    pub created_by: Option<UserId>,
    /// Optional pages of document content
    pub pages: Option<Vec<DocumentPage>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentPage {
    pub page: u64,
    pub content: String,
}

#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateSearchIndexData {
    pub folder_id: FolderId,
    pub name: String,
    pub content: Option<String>,
    pub pages: Option<Vec<DocumentPage>>,
}

/// Search results scoped to a specific file
#[derive(Debug)]
pub struct FileSearchResults {
    // Total number of hits against the item
    pub total_hits: u64,
    /// Matches within the contents
    pub results: Vec<PageResult>,
}

#[derive(Debug)]
pub struct SearchResults {
    pub results: Vec<FlattenedItemResult>,
    pub total_hits: u64,
}

/// Condensed version of a file result
#[derive(Debug)]
pub struct FlattenedItemResult {
    /// Type of item being included in the search index
    pub item_ty: SearchIndexType,
    /// ID of the item itself
    pub item_id: Uuid,
    /// Scope the item is within
    pub document_box: DocumentBoxScopeRaw,
    /// Matches within the page content
    pub page_matches: Vec<PageResult>,
    // Total number of hits against the item
    pub total_hits: u64,
    // Score of the item (Sum of all the content scores)
    pub score: SearchScore,

    /// Whether the content matches
    pub name_match: bool,

    /// Whether the name matches
    pub content_match: bool,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(untagged)]
pub enum SearchScore {
    /// Typesense uses integer scoring
    Integer(u64),
    /// OpenSearch and database use float scoring
    Float(f32),
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PageResult {
    pub page: u64,
    pub matches: Vec<String>,
}

/// Extended search request to search within multiple document
/// boxes
#[derive(Default, Debug, Validate, Deserialize, Serialize, ToSchema)]
#[serde(default)]
pub struct AdminSearchRequest {
    #[garde(skip)]
    #[schema(value_type = Vec<String>)]
    pub scopes: Vec<DocumentBoxScopeRaw>,

    #[serde(flatten)]
    #[garde(dive)]
    pub request: SearchRequest,
}

/// Request to search within a file
#[derive(Default, Debug, Validate, Deserialize, Serialize, ToSchema)]
#[serde(default)]
pub struct FileSearchRequest {
    /// The search query
    #[garde(skip)]
    pub query: Option<String>,

    /// Offset to start returning results from
    #[garde(skip)]
    pub offset: Option<u64>,

    /// Maximum number of results to return
    #[garde(skip)]
    pub limit: Option<u16>,
}

/// Wrapper around [Mime] to implement [Serialize] and [Deserialize]
#[serde_as]
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(transparent)]
pub struct StringMime(#[serde_as(as = "serde_with::DisplayFromStr")] pub Mime);

/// Request to search within a document box
#[derive(Default, Debug, Validate, Deserialize, Serialize, ToSchema)]
#[serde(default)]
pub struct SearchRequest {
    /// The search query
    #[garde(skip)]
    pub query: Option<String>,

    /// Enable searching with AI
    #[garde(skip)]
    pub neural: bool,

    /// Search only include a specific mime type
    #[garde(skip)]
    #[schema(value_type = Option<String>)]
    pub mime: Option<StringMime>,

    /// Whether to include document names
    #[garde(skip)]
    pub include_name: bool,

    /// Whether to include document content
    #[garde(skip)]
    pub include_content: bool,

    /// Creation date range search
    #[garde(dive)]
    pub created_at: Option<SearchRange>,

    /// Search by a created user
    #[garde(skip)]
    pub created_by: Option<UserId>,

    /// Enforce search to a specific folder, empty for all
    /// folders
    #[garde(skip)]
    #[schema(value_type = Option<Uuid>)]
    pub folder_id: Option<FolderId>,

    /// Number of items to include in the response
    #[garde(skip)]
    pub size: Option<u16>,

    /// Offset to start results from
    #[garde(skip)]
    pub offset: Option<u64>,

    /// Maximum number of pages too return per file
    #[garde(range(max = 100))]
    #[schema(maximum = 100)]
    pub max_pages: Option<u16>,

    /// Offset to start at when aggregating page results
    #[garde(skip)]
    pub pages_offset: Option<u64>,
}

#[derive(Default, Debug, Deserialize, Serialize, ToSchema)]
pub struct SearchRange {
    pub start: Option<DateTime<Utc>>,
    pub end: Option<DateTime<Utc>>,
}

impl Validate for SearchRange {
    type Context = ();

    fn validate_into(
        &self,
        _ctx: &Self::Context,
        parent: &mut dyn FnMut() -> garde::Path,
        report: &mut garde::Report,
    ) {
        match (&self.start, &self.end) {
            (None, None) => report.append(
                parent(),
                garde::Error::new("date range must have a start or end point"),
            ),
            (Some(start), Some(end)) => {
                if start > end {
                    report.append(
                        parent().join("start"),
                        garde::Error::new("date range start cannot be after end"),
                    )
                }
            }
            (None, Some(_)) | (Some(_), None) => {}
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(tag = "type")]
pub enum SearchResultData {
    File(FileWithExtra),
    Folder(FolderWithExtra),
    Link(LinkWithExtra),
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SearchResultResponse {
    pub total_hits: u64,
    pub results: Vec<SearchResultItem>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FileSearchResultResponse {
    pub total_hits: u64,
    pub results: Vec<PageResult>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminSearchResultResponse {
    pub total_hits: u64,
    pub results: Vec<WithScope<SearchResultItem>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SearchResultItem {
    /// The result score
    pub score: SearchScore,
    /// Path to the search result item
    pub path: Vec<FolderPathSegment>,
    /// The item itself
    #[serde(flatten)]
    pub data: SearchResultData,

    pub page_matches: Vec<PageResult>,
    pub total_hits: u64,

    pub name_match: bool,
    pub content_match: bool,
}

/// Request to list users
#[derive(Default, Debug, Validate, Deserialize, Serialize, ToSchema)]
#[serde(default)]
pub struct UsersRequest {
    /// Offset to start returning results from
    #[garde(skip)]
    pub offset: Option<u64>,

    /// Number of items to include in the response
    #[garde(skip)]
    pub size: Option<u16>,
}

#[derive(Debug, Serialize)]
pub struct AdminUsersResults {
    /// The users
    pub results: Vec<User>,
    /// The total number of users
    pub total: i64,
}
