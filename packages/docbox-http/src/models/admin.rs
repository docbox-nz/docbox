use docbox_core::database::models::document_box::DocumentBox;
use garde::Validate;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Default, Debug, Validate, Deserialize, Serialize, ToSchema)]
#[serde(default)]
pub struct TenantDocumentBoxesRequest {
    /// Optional query to search document boxes by
    #[garde(skip)]
    pub query: Option<String>,

    /// Number of items to include in the response
    #[garde(skip)]
    pub size: Option<u16>,

    /// Offset to start results from
    #[garde(skip)]
    pub offset: Option<u64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TenantDocumentBoxesResponse {
    /// The document boxes
    pub results: Vec<DocumentBox>,
    /// The total number of document boxes available to query
    pub total: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TenantStatsResponse {
    /// Total number of files within the document box
    pub total_files: i64,
    /// Total number of links within the document box
    pub total_links: i64,
    /// Total number of folders within the document box
    pub total_folders: i64,
    /// Total size of all files within the tenant
    pub file_size: i64,
}
