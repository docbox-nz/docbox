use docbox_database::models::{document_box::DocumentBoxScopeRaw, folder::FolderId, user::UserId};
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use uuid::Uuid;

use crate::models::SearchIndexType;

/// Document entry within the typesense index
#[derive(Serialize, Deserialize)]
pub struct TypesenseEntry {
    /// Unique ID for the entry
    pub id: Uuid,

    /// Entry data
    #[serde(flatten)]
    pub entry: TypesenseDataEntry,
}

/// Wrapper around the entry to support versioning for
/// future changes in structure
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "version")]
pub enum TypesenseDataEntry {
    /// Current V1 entry format
    V1(TypesenseDataEntryV1),
}

/// Type of document entries
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "entry_type")]
pub enum TypesenseDataEntryV1 {
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
pub struct TypesenseDataEntryRootV1 {
    /// Scope the entry is within
    pub document_box: DocumentBoxScopeRaw,
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

/// Page entry for an item page
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
pub struct SearchResponse<T> {
    pub results: Vec<T>,
}

#[derive(Deserialize)]
#[allow(unused)]
pub struct GenericSearchResponse {
    pub found: u64,
    pub hits: Vec<Hit>,
}

#[derive(Deserialize)]
pub struct GroupedSearchResponse {
    pub found: u64,
    pub grouped_hits: Vec<GroupedHits>,
}

#[derive(Deserialize)]
pub struct GroupedHits {
    pub found: u64,
    pub hits: Vec<Hit>,
}

#[derive(Deserialize)]
pub struct Hit {
    pub document: TypesenseDataEntry,
    pub highlights: Vec<Highlight>,
    pub text_match: u64,
}

#[derive(Deserialize)]
pub struct Highlight {
    pub field: String,
    pub snippet: String,
}
