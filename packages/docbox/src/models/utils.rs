use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
pub struct DocboxServerResponse {
    pub version: &'static str,
}
