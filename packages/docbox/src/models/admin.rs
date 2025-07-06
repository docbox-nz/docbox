use docbox_database::models::document_box::DocumentBox;
use garde::Validate;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Default, Debug, Validate, Deserialize, Serialize, ToSchema)]
#[serde(default)]
pub struct TenantDocumentBoxesRequest {
    /// Number of items to include in the response
    #[garde(skip)]
    pub size: Option<u16>,

    /// Offset to start results from
    #[garde(skip)]
    pub offset: Option<u64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TenantDocumentBoxesResponse {
    pub results: Vec<DocumentBox>,
}
