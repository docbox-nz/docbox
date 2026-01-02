use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
pub struct DocboxServerResponse {
    /// Version of the docbox server
    pub version: &'static str,
}
