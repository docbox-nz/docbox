use utoipa::OpenApi;

use crate::routes::task::{self, TASK_TAG};

#[derive(OpenApi)]
#[openapi(
        tags(
            (name = TASK_TAG, description = "Background task related APIs")
        ),
        paths(task::get)
    )]
pub struct ApiDoc;

#[test]
#[ignore = "generates documentation"]
fn generate_api_docs() {
    println!("{}", ApiDoc::openapi().to_pretty_json().unwrap());
}
