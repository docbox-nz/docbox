use utoipa::OpenApi;

use crate::{
    models::document_box::DocumentBoxScope,
    routes::{
        admin::{self, ADMIN_TAG},
        document_box::{self, DOCUMENT_BOX_TAG},
        file::{self, FILE_TAG},
        folder::{self, FOLDER_TAG},
        link::{self, LINK_TAG},
        task::{self, TASK_TAG},
        utils::{self, UTILS_TAG},
    },
};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Docbox API",
        description = "Docbox HTTP API",
        license(
            name = "MIT",
            url = "https://raw.githubusercontent.com/docbox-nz/docbox/refs/heads/main/LICENSE.md",
        )
    ),
    tags(
        (name = DOCUMENT_BOX_TAG, description = "Document box related APIs"),
        (name = FILE_TAG, description = "File related APIs"),
        (name = LINK_TAG, description = "Link related APIs"),
        (name = FOLDER_TAG, description = "Folder related APIs"),
        (name = TASK_TAG, description = "Background task related APIs"),
        (name = ADMIN_TAG, description = "Administrator and higher privilege APIs"),
        (name = UTILS_TAG, description = "Utility APIs")
    ),
    components(
        schemas(DocumentBoxScope)
    ),
    paths(
        // Admin routes
        admin::tenant_boxes,
        admin::search_tenant,
        admin::reprocess_octet_stream_files_tenant,
        admin::rebuild_search_index_tenant,
        admin::flush_database_pool_cache,
        admin::flush_tenant_cache,
        admin::http_purge_expired_presigned_tasks,
        // Document box routes
        document_box::create,
        document_box::get,
        document_box::stats,
        document_box::delete,
        document_box::search,
        // File routes
        file::upload,
        file::create_presigned,
        file::get_presigned,
        file::get,
        file::get_children,
        file::get_edit_history,
        file::update,
        file::get_raw,
        file::get_raw_presigned,
        file::get_raw_named,
        file::delete,
        file::get_generated,
        file::get_generated_raw,
        file::get_generated_raw_presigned,
        file::get_generated_raw_named,
        file::search,
        // Folder routes
        folder::create,
        folder::get,
        folder::get_edit_history,
        folder::update,
        folder::delete,
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
        task::get,
        // Utils routes
        utils::get_options,
        utils::health
    )
)]
#[allow(unused)]
pub struct ApiDoc;

#[test]
#[ignore = "generates api documentation"]
fn generate_api_docs() {
    let docs = ApiDoc::openapi().to_pretty_json().unwrap();
    std::fs::write("docbox.json", docs).unwrap();
}
