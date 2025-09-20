use docbox_database::models::{document_box::DocumentBoxScopeRaw, folder::FolderId, user::UserId};
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use uuid::Uuid;

use crate::models::{DocumentPage, SearchIndexType};

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

#[derive(Debug, Deserialize)]
pub struct SearchResponse {
    pub hits: Hits<SearchResponseHit>,
}

#[derive(Debug, Deserialize)]
pub struct Hits<H> {
    pub total: HitsTotal,
    pub hits: Vec<H>,
}

#[derive(Debug, Deserialize)]
pub struct HitsTotal {
    pub value: u64,
}

#[derive(Debug, Deserialize)]
pub struct SearchResponseHit {
    pub _id: String,
    pub _score: f32,
    pub _source: SearchResponseHitSource,
    pub inner_hits: Option<InnerHits>,
    pub matched_queries: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct SearchResponseHitSource {
    pub item_id: Uuid,
    pub item_type: SearchIndexType,
    pub document_box: DocumentBoxScopeRaw,
}

#[derive(Debug, Deserialize)]
pub struct InnerHits {
    pub pages: InnerHitsPages,
}

#[derive(Debug, Deserialize)]
pub struct InnerHitsPages {
    pub hits: Hits<PagesHit>,
}

#[derive(Debug, Deserialize)]
pub struct PagesHit {
    pub _score: f32,
    pub _source: PagesHitSource,
    pub highlight: PagesHighlight,
}

#[derive(Debug, Deserialize)]
pub struct PagesHitSource {
    pub page: u64,
}

#[derive(Debug, Deserialize)]
pub struct PagesHighlight {
    #[serde(rename = "pages.content")]
    pub content: Vec<String>,
}
