use utoipa::OpenApi;

use crate::routes::{
    link::{self, LINK_TAG},
    task::{self, TASK_TAG},
};

#[derive(OpenApi)]
#[openapi(
    tags(
        (name = LINK_TAG, description = "Link related APIs"),
        (name = TASK_TAG, description = "Background task related APIs")
    ),
    paths(
        // Link routes
        link::create, 
        link::get,
        link::get_metadata,
        link::get_favicon,
        link::get_image,
        link::get_edit_history,
        link::update,
        link::delete,
        // Task routes
        task::get
    )
)]
#[allow(unused)]
pub struct ApiDoc;

#[test]
#[ignore = "generates documentation"]
fn generate_api_docs() {
    let docs = ApiDoc::openapi().to_pretty_json().unwrap();
    std::fs::write("docbox.json", docs).unwrap();
}
