use utoipa::OpenApi;

use crate::routes::{
    link::{self, LINK_TAG},
    task::{self, TASK_TAG},
    folder::{self, FOLDER_TAG},
    document_box::{self, DOCUMENT_BOX_TAG}
};

#[derive(OpenApi)]
#[openapi(
    tags(
        (name = DOCUMENT_BOX_TAG, description = "Document box related APIs"),
        (name = LINK_TAG, description = "Link related APIs"),
        (name = FOLDER_TAG, description = "Folder related APIs"),
        (name = TASK_TAG, description = "Background task related APIs")
    ),
    paths(
        // Document box routes
        document_box::create,
        document_box::get,
        document_box::stats,
        document_box::delete,
        document_box::search,
        // Link routes
        link::create, 
        link::get,
        link::get_metadata,
        link::get_favicon,
        link::get_image,
        link::get_edit_history,
        link::update,
        link::delete,
        // Folder routes
        folder::create,
        folder::get,
        folder::get_edit_history,
        folder::update,
        folder::delete,
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
