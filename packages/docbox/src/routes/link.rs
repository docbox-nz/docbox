//! Link related endpoints

use crate::error::{HttpCommonError, HttpErrorResponse};
use crate::middleware::action_user::UserParams;
use crate::middleware::tenant::TenantParams;
use crate::{
    error::{DynHttpError, HttpResult, HttpStatusResult},
    middleware::{
        action_user::ActionUser,
        tenant::{TenantDb, TenantEvents, TenantSearch},
    },
    models::{
        folder::HttpFolderError,
        link::{CreateLink, HttpLinkError, LinkMetadataResponse, UpdateLinkRequest},
    },
};
use axum::http::header;
use axum::{
    body::Body,
    extract::Path,
    http::{Response, StatusCode},
    Extension, Json,
};
use axum_valid::Garde;
use docbox_core::search::models::UpdateSearchIndexData;
use docbox_core::services::links::{
    delete_link, move_link, safe_create_link, update_link_name, update_link_value, CreateLinkData,
};
use docbox_database::models::{
    document_box::DocumentBoxScope,
    edit_history::EditHistory,
    folder::Folder,
    link::{CreatedByUser, LastModifiedByUser, Link, LinkId, LinkWithExtra},
};
use docbox_web_scraper::WebsiteMetaService;
use std::{ops::DerefMut, sync::Arc};

pub const LINK_TAG: &str = "Link";

/// Create link
///
/// Creates a new link within the provided document box
#[utoipa::path(
    post,
    operation_id = "link_create",
    tag = LINK_TAG,
    path = "/box/{scope}/link",
    responses(
        (status = 201, description = "Link created successfully", body = LinkWithExtra),
        (status = 404, description = "Destination folder not found", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(
        ("scope" = String, Path, description = "Scope to create the link within"),
        TenantParams,
        UserParams
    )
)]
#[tracing::instrument(skip_all, fields(scope, req))]
pub async fn create(
    action_user: ActionUser,
    TenantDb(db): TenantDb,
    TenantSearch(search): TenantSearch,
    TenantEvents(events): TenantEvents,
    Path(scope): Path<DocumentBoxScope>,
    Garde(Json(req)): Garde<Json<CreateLink>>,
) -> Result<(StatusCode, Json<LinkWithExtra>), DynHttpError> {
    let folder_id = req.folder_id;
    let folder = Folder::find_by_id(&db, &scope, folder_id)
        .await
        // Failed to query destination folder
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query link destination folder");
            HttpCommonError::ServerError
        })?
        // Destination folder was not found
        .ok_or(HttpFolderError::UnknownFolder)?;

    // Update stored editing user data
    let created_by = action_user.store_user(&db).await?;

    // Make the create query
    let create = CreateLinkData {
        folder,
        name: req.name,
        value: req.value,
        created_by: created_by.as_ref().map(|value| value.id.to_string()),
    };

    // Perform Link creation
    let link = safe_create_link(&db, search, &events, create)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to create link");
            HttpLinkError::CreateError(cause)
        })?;

    Ok((
        StatusCode::CREATED,
        Json(LinkWithExtra {
            id: link.id,
            name: link.name,
            value: link.value,
            folder_id: link.folder_id,
            created_at: link.created_at,
            created_by: CreatedByUser(created_by),
            last_modified_at: None,
            last_modified_by: LastModifiedByUser(None),
        }),
    ))
}

/// Get link by ID
///
/// Request a specific link by ID
#[utoipa::path(
    get,
    operation_id = "link_get",
    tag = LINK_TAG,
    path = "/box/{scope}/link/{link_id}",
    responses(
        (status = 200, description = "Link obtained successfully", body = LinkWithExtra),
        (status = 404, description = "Link not found", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(
        ("scope" = String, Path, description = "Scope the link resides within"),
        ("link_id" = Uuid, Path, description = "ID of the link to request"),
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(scope, link_id))]
pub async fn get(
    TenantDb(db): TenantDb,
    Path((scope, link_id)): Path<(DocumentBoxScope, LinkId)>,
) -> HttpResult<LinkWithExtra> {
    let link = Link::find_with_extra(&db, &scope, link_id)
        .await
        // Failed to query link
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query link");
            HttpCommonError::ServerError
        })?
        // Link not found
        .ok_or(HttpLinkError::UnknownLink)?;

    Ok(Json(link))
}

/// Get link website metadata
///
/// Requests metadata for the link. This will make a request
/// to the site at the link value to extract metadata from
/// the website itself such as title, and OGP metadata
#[utoipa::path(
    get,
    operation_id = "link_get_metadata",
    tag = LINK_TAG,
    path = "/box/{scope}/link/{link_id}/metadata",
    responses(
        (status = 200, description = "Obtained link metadata successfully", body = LinkWithExtra),
        (status = 404, description = "Link not found or failed to resolve metadata", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(
        ("scope" = String, Path, description = "Scope the link resides within"),
        ("link_id" = Uuid, Path, description = "ID of the link to request"),
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(scope, link_id))]
pub async fn get_metadata(
    TenantDb(db): TenantDb,
    Extension(website_service): Extension<Arc<WebsiteMetaService>>,
    Path((scope, link_id)): Path<(DocumentBoxScope, LinkId)>,
) -> HttpResult<LinkMetadataResponse> {
    let link = Link::find_with_extra(&db, &scope, link_id)
        .await
        // Failed to query link
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query link");
            HttpCommonError::ServerError
        })?
        // Link not found
        .ok_or(HttpLinkError::UnknownLink)?;

    let resolved = website_service
        .resolve_website(&link.value)
        .await
        .map_err(|cause| {
            tracing::warn!(?cause, "failed to resolve link site metadata");
            HttpLinkError::FailedResolve
        })?;

    Ok(Json(LinkMetadataResponse {
        title: resolved.title,
        og_title: resolved.og_title,
        og_description: resolved.og_description,
        favicon: resolved.favicon.is_some(),
        image: resolved.og_image.is_some(),
    }))
}

/// Get link favicon
///
/// Obtain the favicon image for the website that
/// the link points to
#[utoipa::path(
    get,
    operation_id = "link_get_favicon",
    tag = LINK_TAG,
    path = "/box/{scope}/link/{link_id}/favicon",
    responses(
        (status = 200, description = "Obtained link favicon", body = LinkWithExtra),
        (status = 404, description = "Link not found or no favicon was found", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(
        ("scope" = String, Path, description = "Scope the link resides within"),
        ("link_id" = Uuid, Path, description = "ID of the link to request"),
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(scope, link_id))]
pub async fn get_favicon(
    TenantDb(db): TenantDb,
    Extension(website_service): Extension<Arc<WebsiteMetaService>>,
    Path((scope, link_id)): Path<(DocumentBoxScope, LinkId)>,
) -> Result<Response<Body>, DynHttpError> {
    let link = Link::find_with_extra(&db, &scope, link_id)
        .await
        // Failed to query link
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query link");
            HttpCommonError::ServerError
        })?
        // Link not found
        .ok_or(HttpLinkError::UnknownLink)?;

    let resolved = website_service
        .resolve_website(&link.value)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to resolve website");
            HttpCommonError::ServerError
        })?;
    let favicon = resolved.favicon.ok_or(HttpLinkError::NoFavicon)?;
    let body = axum::body::Body::from(favicon.bytes);

    Ok(Response::builder()
        .header(header::CONTENT_TYPE, favicon.content_type.to_string())
        .body(body)?)
}

/// Get link social image
///
/// Obtain the "Social Image" for the website, this resolves the website
/// metadata and finds the OGP metadata image responding with the image
/// directly
#[utoipa::path(
    get,
    operation_id = "link_get_image",
    tag = LINK_TAG,
    path = "/box/{scope}/link/{link_id}/image",
    responses(
        (status = 200, description = "Obtained link social image", body = LinkWithExtra),
        (status = 404, description = "Link not found or no image was found", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(
        ("scope" = String, Path, description = "Scope the link resides within"),
        ("link_id" = Uuid, Path, description = "ID of the link to request"),
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(scope, link_id))]
pub async fn get_image(
    TenantDb(db): TenantDb,
    Extension(website_service): Extension<Arc<WebsiteMetaService>>,
    Path((scope, link_id)): Path<(DocumentBoxScope, LinkId)>,
) -> Result<Response<Body>, DynHttpError> {
    let link = Link::find_with_extra(&db, &scope, link_id)
        .await
        // Failed to query link
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query link");
            HttpCommonError::ServerError
        })?
        // Link not found
        .ok_or(HttpLinkError::UnknownLink)?;

    let resolved = website_service
        .resolve_website(&link.value)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to resolve website");
            HttpCommonError::ServerError
        })?;
    let og_image = resolved.og_image.ok_or(HttpLinkError::NoImage)?;
    let body = axum::body::Body::from(og_image.bytes);

    Ok(Response::builder()
        .header(header::CONTENT_TYPE, og_image.content_type.to_string())
        .body(body)?)
}

/// Get link edit history
///
/// Request the edit history for the provided link
#[utoipa::path(
    get,
    operation_id = "link_get_edit_history",
    tag = LINK_TAG,
    path = "/box/{scope}/link/{link_id}/edit-history",
    responses(
        (status = 200, description = "Obtained edit history", body = [EditHistory]),
        (status = 404, description = "Link not found", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(
        ("scope" = String, Path, description = "Scope the link resides within"),
        ("link_id" = Uuid, Path, description = "ID of the link to request"),
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(scope, link_id))]
pub async fn get_edit_history(
    TenantDb(db): TenantDb,
    Path((scope, link_id)): Path<(DocumentBoxScope, LinkId)>,
) -> HttpResult<Vec<EditHistory>> {
    // Ensure the link itself exists
    _ = Link::find(&db, &scope, link_id)
        .await
        // Failed to query link
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query link");
            HttpCommonError::ServerError
        })?
        // Link not found
        .ok_or(HttpLinkError::UnknownLink)?;

    let history = EditHistory::all_by_link(&db, link_id)
        .await
        // Failed to query edit history
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query link edit history");
            HttpCommonError::ServerError
        })?;

    Ok(Json(history))
}

/// Update link
///
/// Updates a link, can be a name change, value change, a folder move, or all
#[utoipa::path(
    put,
    operation_id = "link_update",
    tag = LINK_TAG,
    path = "/box/{scope}/link/{link_id}",
    responses(
        (status = 200, description = "Updated link successfully"),
        (status = 404, description = "Link not found", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(
        ("scope" = String, Path, description = "Scope the link resides within"),
        ("link_id" = Uuid, Path, description = "ID of the link to request"),
        TenantParams,
        UserParams
    )
)]
#[tracing::instrument(skip_all, fields(scope, link_id, req))]
pub async fn update(
    action_user: ActionUser,
    TenantDb(db): TenantDb,
    TenantSearch(opensearch): TenantSearch,
    Path((scope, link_id)): Path<(DocumentBoxScope, LinkId)>,
    Garde(Json(req)): Garde<Json<UpdateLinkRequest>>,
) -> HttpStatusResult {
    let mut link = Link::find(&db, &scope, link_id)
        .await
        // Failed to query link
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query link");
            HttpCommonError::ServerError
        })?
        // Link not found
        .ok_or(HttpLinkError::UnknownLink)?;

    let mut db = db.begin().await.map_err(|cause| {
        tracing::error!(?cause, "failed to begin transaction");
        HttpCommonError::ServerError
    })?;

    // Update stored editing user data
    let user = action_user.store_user(db.deref_mut()).await?;
    let user_id = user.as_ref().map(|value| value.id.to_string());

    if let Some(target_id) = req.folder_id {
        // Ensure the target folder exists, also ensures the target folder is in the same scope
        // (We may allow across scopes in the future, but would need additional checks for access control of target scope)
        let target_folder = Folder::find_by_id(db.deref_mut(), &scope, target_id)
            .await
            .map_err(|cause| {
                tracing::error!(?cause, "failed to query target folder");
                HttpCommonError::ServerError
            })?
            .ok_or(HttpFolderError::UnknownTargetFolder)?;

        link = move_link(&mut db, user_id.clone(), link, target_folder)
            .await
            .map_err(|cause| {
                tracing::error!(?cause, "failed to move link");
                HttpCommonError::ServerError
            })?;
    };

    if let Some(new_name) = req.name {
        link = update_link_name(&mut db, user_id.clone(), link, new_name)
            .await
            .map_err(|cause| {
                tracing::error!(?cause, "failed to update link name");
                HttpCommonError::ServerError
            })?;
    }

    if let Some(new_value) = req.value {
        link = update_link_value(&mut db, user_id, link, new_value)
            .await
            .map_err(|cause| {
                tracing::error!(?cause, "failed to update link value");
                HttpCommonError::ServerError
            })?;
    }

    // Update search index data for the new name and value
    opensearch
        .update_data(
            link.id,
            UpdateSearchIndexData {
                folder_id: Some(link.folder_id),
                name: Some(link.name.clone()),
                mime: None,
                content: Some(link.value.clone()),
                created_at: None,
                created_by: None,
                document_box: None,
                pages: None,
            },
        )
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to update search index");
            HttpCommonError::ServerError
        })?;

    db.commit().await.map_err(|cause| {
        tracing::error!(?cause, "failed to commit transaction");
        HttpCommonError::ServerError
    })?;

    Ok(StatusCode::OK)
}

/// Delete a link by ID
///
/// Deletes a specific link using its ID
#[utoipa::path(
    delete,
    operation_id = "link_delete",
    tag = LINK_TAG,
    path = "/box/{scope}/link/{link_id}",
    responses(
        (status = 204, description = "Deleted link successfully"),
        (status = 404, description = "Link not found", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(
        ("scope" = String, Path, description = "Scope the link resides within"),
        ("link_id" = Uuid, Path, description = "ID of the link to delete"),
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(scope, link_id))]
pub async fn delete(
    TenantDb(db): TenantDb,
    TenantSearch(opensearch): TenantSearch,
    TenantEvents(events): TenantEvents,
    Path((scope, link_id)): Path<(DocumentBoxScope, LinkId)>,
) -> HttpStatusResult {
    let link = Link::find(&db, &scope, link_id)
        .await
        // Failed to query link
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query link");
            HttpCommonError::ServerError
        })?
        // Link not found
        .ok_or(HttpLinkError::UnknownLink)?;

    delete_link(&db, &opensearch, &events, link, scope)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to delete folder");
            HttpCommonError::ServerError
        })?;

    Ok(StatusCode::NO_CONTENT)
}
