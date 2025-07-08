use axum::{
    Router,
    routing::{get, post},
};

use super::middleware::tenant::tenant_auth_middleware;

pub mod admin;
pub mod document_box;
pub mod file;
pub mod folder;
pub mod link;
pub mod task;
pub mod utils;

pub fn router() -> Router {
    Router::new()
        .nest("/admin", admin_router())
        .nest("/box", document_box_router())
        .route("/options", get(utils::get_options))
}

/// Routes for /admin/
pub fn admin_router() -> Router {
    Router::new()
        .route("/flush-db-cache", post(admin::flush_database_pool_cache))
        .route(
            "/boxes",
            post(admin::tenant_boxes).layer(axum::middleware::from_fn(tenant_auth_middleware)),
        )
        .route(
            "/search",
            post(admin::search_tenant).layer(axum::middleware::from_fn(tenant_auth_middleware)),
        )
        .route(
            "/purge-expired-presigned-tasks",
            post(admin::http_purge_expired_presigned_tasks),
        )
}

/// Routes for /box/
pub fn document_box_router() -> Router {
    Router::new()
        .route("/", post(document_box::create))
        .nest(
            "/{scope}",
            Router::new()
                .route("/", get(document_box::get).delete(document_box::delete))
                .route("/stats", get(document_box::stats))
                .route("/search", post(document_box::search))
                .nest("/file", file_router())
                .nest("/task", task_router())
                .nest("/link", link_router())
                .nest("/folder", folder_router()),
        )
        // Layer to authorize requests
        .layer(axum::middleware::from_fn(tenant_auth_middleware))
}

/// Routes for /box/:scope/folder/
pub fn folder_router() -> Router {
    Router::new().route("/", post(folder::create)).nest(
        "/{folder_id}",
        Router::new()
            .route(
                "/",
                get(folder::get).put(folder::update).delete(folder::delete),
            )
            .route("/edit-history", get(folder::get_edit_history)),
    )
}

/// Routes for /box/:scope/file/
pub fn file_router() -> Router {
    Router::new()
        .route("/", post(file::upload))
        .nest(
            "/presigned",
            Router::new()
                .route("/", post(file::create_presigned))
                .route("/{task_id}", get(file::get_presigned)),
        )
        .nest(
            "/{file_id}",
            Router::new()
                .route("/", get(file::get).put(file::update).delete(file::delete))
                .route("/raw", get(file::get_raw))
                .route("/raw-presigned", post(file::get_raw_presigned))
                // Named access endpoint, allows specifying some file name after the URL
                // (Used to work around a Chromium bug which makes inline viewers not respect the filename)
                .route("/raw/{*name}", get(file::get_raw_named))
                .route("/children", get(file::get_children))
                .route("/edit-history", get(file::get_edit_history))
                .route("/search", post(file::search))
                // Generated file instance
                .nest(
                    "/generated",
                    Router::new().nest(
                        "/{generated_type}",
                        Router::new()
                            .route("/", get(file::get_generated))
                            .route("/raw", get(file::get_generated_raw))
                            .route("/raw-presigned", post(file::get_generated_raw_presigned))
                            // Named access endpoint, allows specifying some file name after the URL
                            // (Used to work around a Chromium bug which makes inline viewers not respect the filename)
                            .route("/raw/{*name}", get(file::get_generated_raw_named)),
                    ),
                ),
        )
}

/// Routes for /box/:scope/task/
pub fn task_router() -> Router {
    Router::new().nest("/{task_id}", Router::new().route("/", get(task::get)))
}

/// Routes for /box/:scope/link/
pub fn link_router() -> Router {
    Router::new().route("/", post(link::create)).nest(
        "/{link_id}",
        Router::new()
            .route("/", get(link::get).put(link::update).delete(link::delete))
            .route("/metadata", get(link::get_metadata))
            .route("/favicon", get(link::get_favicon))
            .route("/image", get(link::get_image))
            .route("/edit-history", get(link::get_edit_history)),
    )
}
