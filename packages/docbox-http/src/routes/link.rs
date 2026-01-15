//! Link related endpoints

use crate::{
    error::{DynHttpError, HttpCommonError, HttpErrorResponse, HttpResult, HttpStatusResult},
    middleware::{
        action_user::{ActionUser, UserParams},
        tenant::{TenantDb, TenantEvents, TenantParams, TenantSearch},
    },
    models::{
        document_box::DocumentBoxScope,
        file::BinaryResponse,
        folder::HttpFolderError,
        link::{CreateLink, HttpLinkError, LinkMetadataResponse, UpdateLinkRequest},
    },
};
use axum::{
    Extension, Json,
    body::Body,
    extract::Path,
    http::{Response, StatusCode, header},
};
use axum_valid::Garde;
use docbox_core::database::models::{
    edit_history::EditHistory,
    folder::Folder,
    link::{Link, LinkId, LinkWithExtra},
};
use docbox_core::links::{
    create_link::{CreateLinkData, safe_create_link},
    delete_link::delete_link,
    resolve_website::ResolveWebsiteService,
    update_link::{UpdateLink, UpdateLinkError},
};
use std::sync::Arc;

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
        ("scope" = DocumentBoxScope, Path, description = "Scope to create the link within"),
        TenantParams,
        UserParams
    )
)]
#[tracing::instrument(skip_all, fields(%scope))]
pub async fn create(
    action_user: ActionUser,
    TenantDb(db): TenantDb,
    TenantSearch(search): TenantSearch,
    TenantEvents(events): TenantEvents,
    Path(DocumentBoxScope(scope)): Path<DocumentBoxScope>,
    Garde(Json(req)): Garde<Json<CreateLink>>,
) -> Result<(StatusCode, Json<LinkWithExtra>), DynHttpError> {
    let folder_id = req.folder_id;
    let folder = Folder::find_by_id(&db, &scope, folder_id)
        .await
        // Failed to query destination folder
        .map_err(|error| {
            tracing::error!(?error, "failed to query link destination folder");
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
        .map_err(|error| {
            tracing::error!(?error, "failed to create link");
            HttpLinkError::CreateError(error)
        })?;

    Ok((
        StatusCode::CREATED,
        Json(LinkWithExtra {
            link,
            created_by,
            last_modified_at: None,
            last_modified_by: None,
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
        ("scope" = DocumentBoxScope, Path, description = "Scope the link resides within"),
        ("link_id" = Uuid, Path, description = "ID of the link to request"),
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(%scope, %link_id))]
pub async fn get(
    TenantDb(db): TenantDb,
    Path((scope, link_id)): Path<(DocumentBoxScope, LinkId)>,
) -> HttpResult<LinkWithExtra> {
    let DocumentBoxScope(scope) = scope;

    let link = Link::find_with_extra(&db, &scope, link_id)
        .await
        // Failed to query link
        .map_err(|error| {
            tracing::error!(?error, "failed to query link");
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
        ("scope" = DocumentBoxScope, Path, description = "Scope the link resides within"),
        ("link_id" = Uuid, Path, description = "ID of the link to request"),
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(%scope, %link_id))]
pub async fn get_metadata(
    TenantDb(db): TenantDb,
    Extension(website_service): Extension<Arc<ResolveWebsiteService>>,
    Path((scope, link_id)): Path<(DocumentBoxScope, LinkId)>,
) -> HttpResult<LinkMetadataResponse> {
    let DocumentBoxScope(scope) = scope;

    let link = Link::find(&db, &scope, link_id)
        .await
        // Failed to query link
        .map_err(|error| {
            tracing::error!(?error, "failed to query link");
            HttpCommonError::ServerError
        })?
        // Link not found
        .ok_or(HttpLinkError::UnknownLink)?;

    let url = docbox_core::web_scraper::Url::parse(&link.value).map_err(|error| {
        tracing::warn!(?error, "invalid website");
        HttpLinkError::InvalidLinkUrl
    })?;

    let resolved = website_service
        .resolve_website(&db, &url)
        .await
        .ok_or_else(|| {
            tracing::warn!("failed to resolve link site metadata");
            HttpLinkError::FailedResolve
        })?;

    Ok(Json(LinkMetadataResponse {
        title: resolved.title,
        og_title: resolved.og_title,
        og_description: resolved.og_description,
        favicon: resolved.best_favicon.is_some(),
        image: resolved.og_image.is_some(),
    }))
}

/// Get link favicon
///
/// Obtain the favicon image for the website that the link points to
/// the image data is streamed directly from the target website
#[utoipa::path(
    get,
    operation_id = "link_get_favicon",
    tag = LINK_TAG,
    path = "/box/{scope}/link/{link_id}/favicon",
    responses(
        (status = 200, description = "Streamed link favicon binary data", content_type = "application/octet-stream", body = BinaryResponse),
        (status = 404, description = "Link not found or no favicon was found", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(
        ("scope" = DocumentBoxScope, Path, description = "Scope the link resides within"),
        ("link_id" = Uuid, Path, description = "ID of the link to request"),
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(%scope, %link_id))]
pub async fn get_favicon(
    TenantDb(db): TenantDb,
    Extension(website_service): Extension<Arc<ResolveWebsiteService>>,
    Path((scope, link_id)): Path<(DocumentBoxScope, LinkId)>,
) -> Result<Response<Body>, DynHttpError> {
    let DocumentBoxScope(scope) = scope;

    let link = Link::find(&db, &scope, link_id)
        .await
        // Failed to query link
        .map_err(|error| {
            tracing::error!(?error, "failed to query link");
            HttpCommonError::ServerError
        })?
        // Link not found
        .ok_or(HttpLinkError::UnknownLink)?;

    let url = docbox_core::web_scraper::Url::parse(&link.value).map_err(|error| {
        tracing::warn!(?error, "invalid website");
        HttpLinkError::InvalidLinkUrl
    })?;

    let website_metadata = website_service
        .resolve_website(&db, &url)
        .await
        .ok_or(HttpLinkError::NoFavicon)?;

    let favicon = website_service
        .service
        .resolve_favicon(&url, website_metadata.best_favicon)
        .await
        .ok_or(HttpLinkError::NoFavicon)?;

    let body = axum::body::Body::from_stream(favicon.stream);

    Ok(Response::builder()
        .header(header::CONTENT_TYPE, favicon.content_type.to_string())
        .header(
            header::CONTENT_SECURITY_POLICY,
            "default-src 'none'; img-src 'self' data:;",
        )
        .header(
            header::CACHE_CONTROL,
            "public, max-age=3600, stale-while-revalidate=86400",
        )
        .body(body)?)
}

/// Get link social image
///
/// Obtain the "Social Image" for the website, this resolves the website
/// metadata and finds the OGP metadata image responding with the image
/// directly. The image data is streamed directly from the target
/// website
#[utoipa::path(
    get,
    operation_id = "link_get_image",
    tag = LINK_TAG,
    path = "/box/{scope}/link/{link_id}/image",
    responses(
        (status = 200, description = "Streamed link social image binary data", content_type = "application/octet-stream", body = BinaryResponse),
        (status = 404, description = "Link not found or no image was found", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(
        ("scope" = DocumentBoxScope, Path, description = "Scope the link resides within"),
        ("link_id" = Uuid, Path, description = "ID of the link to request"),
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(%scope, %link_id))]
pub async fn get_image(
    TenantDb(db): TenantDb,
    Extension(website_service): Extension<Arc<ResolveWebsiteService>>,
    Path((scope, link_id)): Path<(DocumentBoxScope, LinkId)>,
) -> Result<Response<Body>, DynHttpError> {
    let DocumentBoxScope(scope) = scope;

    let link = Link::find(&db, &scope, link_id)
        .await
        // Failed to query link
        .map_err(|error| {
            tracing::error!(?error, "failed to query link");
            HttpCommonError::ServerError
        })?
        // Link not found
        .ok_or(HttpLinkError::UnknownLink)?;

    let url = docbox_core::web_scraper::Url::parse(&link.value).map_err(|error| {
        tracing::warn!(?error, "invalid website");
        HttpLinkError::InvalidLinkUrl
    })?;

    let website_metadata = website_service
        .resolve_website(&db, &url)
        .await
        .ok_or(HttpLinkError::NoImage)?;

    let og_image = website_metadata.og_image.ok_or(HttpLinkError::NoImage)?;
    let og_image = website_service
        .service
        .resolve_image(&url, &og_image)
        .await
        .ok_or(HttpLinkError::NoImage)?;

    let body = axum::body::Body::from_stream(og_image.stream);

    Ok(Response::builder()
        .header(header::CONTENT_TYPE, og_image.content_type.to_string())
        .header(
            header::CONTENT_SECURITY_POLICY,
            "default-src 'none'; img-src 'self' data:;",
        )
        .header(
            header::CACHE_CONTROL,
            "public, max-age=3600, stale-while-revalidate=86400",
        )
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
        ("scope" = DocumentBoxScope, Path, description = "Scope the link resides within"),
        ("link_id" = Uuid, Path, description = "ID of the link to request"),
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(%scope, %link_id))]
pub async fn get_edit_history(
    TenantDb(db): TenantDb,
    Path((scope, link_id)): Path<(DocumentBoxScope, LinkId)>,
) -> HttpResult<Vec<EditHistory>> {
    let DocumentBoxScope(scope) = scope;

    // Ensure the link itself exists
    _ = Link::find(&db, &scope, link_id)
        .await
        // Failed to query link
        .map_err(|error| {
            tracing::error!(?error, "failed to query link");
            HttpCommonError::ServerError
        })?
        // Link not found
        .ok_or(HttpLinkError::UnknownLink)?;

    let history = EditHistory::all_by_link(&db, link_id)
        .await
        // Failed to query edit history
        .map_err(|error| {
            tracing::error!(?error, "failed to query link edit history");
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
        ("scope" = DocumentBoxScope, Path, description = "Scope the link resides within"),
        ("link_id" = Uuid, Path, description = "ID of the link to request"),
        TenantParams,
        UserParams
    )
)]
#[tracing::instrument(skip_all, fields(%scope, %link_id, ?req))]
pub async fn update(
    action_user: ActionUser,
    TenantDb(db): TenantDb,
    TenantSearch(search): TenantSearch,
    Path((scope, link_id)): Path<(DocumentBoxScope, LinkId)>,
    Garde(Json(req)): Garde<Json<UpdateLinkRequest>>,
) -> HttpStatusResult {
    let DocumentBoxScope(scope) = scope;

    let link = Link::find(&db, &scope, link_id)
        .await
        // Failed to query link
        .map_err(|error| {
            tracing::error!(?error, "failed to query link");
            HttpCommonError::ServerError
        })?
        // Link not found
        .ok_or(HttpLinkError::UnknownLink)?;

    // Update stored editing user data
    let user = action_user.store_user(&db).await?;
    let user_id = user.as_ref().map(|value| value.id.to_string());

    let update = UpdateLink {
        folder_id: req.folder_id,
        name: req.name,
        value: req.value,
        pinned: req.pinned,
    };

    docbox_core::links::update_link::update_link(&db, &search, &scope, link, user_id, update)
        .await
        .map_err(|error| match error {
            UpdateLinkError::UnknownTargetFolder => {
                DynHttpError::from(HttpFolderError::UnknownTargetFolder)
            }
            _ => DynHttpError::from(HttpCommonError::ServerError),
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
        ("scope" = DocumentBoxScope, Path, description = "Scope the link resides within"),
        ("link_id" = Uuid, Path, description = "ID of the link to delete"),
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(%scope, %link_id))]
pub async fn delete(
    TenantDb(db): TenantDb,
    TenantSearch(search): TenantSearch,
    TenantEvents(events): TenantEvents,
    Path((scope, link_id)): Path<(DocumentBoxScope, LinkId)>,
) -> HttpStatusResult {
    let DocumentBoxScope(scope) = scope;

    let link = Link::find(&db, &scope, link_id)
        .await
        // Failed to query link
        .map_err(|error| {
            tracing::error!(?error, "failed to query link");
            HttpCommonError::ServerError
        })?
        // Link not found
        .ok_or(HttpLinkError::UnknownLink)?;

    delete_link(&db, &search, &events, link, scope)
        .await
        .map_err(|error| {
            tracing::error!(?error, "failed to delete folder");
            HttpCommonError::ServerError
        })?;

    Ok(StatusCode::NO_CONTENT)
}
