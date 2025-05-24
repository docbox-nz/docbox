//! # Opensearch Models
//!
//! This file contains the models for deserializing search responses
//! from Opensearch

use super::models::SearchIndexType;
use docbox_database::models::document_box::DocumentBoxScope;
use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct SearchResponse {
    pub hits: Hits<SearchResponseHit>,
}

#[derive(Debug, Deserialize)]
pub struct Hits<H> {
    pub total: HitsTotal,
    pub max_score: Option<f64>,
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
    pub document_box: DocumentBoxScope,
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
    pub matched_queries: Option<Vec<String>>,
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
