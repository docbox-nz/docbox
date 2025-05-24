//! Link related endpoints

use std::{ops::DerefMut, sync::Arc};

use anyhow::Context;
use axum::http::header;
use axum::{
    body::Body,
    extract::Path,
    http::{Response, StatusCode},
    Extension, Json,
};
use axum_valid::Garde;
use docbox_core::search::models::UpdateSearchIndexData;

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
use docbox_core::services::links::{
    create_link, delete_link, move_link, rollback_create_link, update_link_name, update_link_value,
    CreateLinkData, CreateLinkState,
};
use docbox_database::models::{
    document_box::DocumentBoxScope,
    edit_history::EditHistory,
    folder::Folder,
    link::{CreatedByUser, LastModifiedByUser, Link, LinkId, LinkWithExtra},
};
use docbox_web_scraper::WebsiteMetaService;

/// POST /box/:scope/link
///
/// Creates a new link in the provided document box folder
pub async fn create(
    action_user: ActionUser,
    TenantDb(db): TenantDb,
    TenantSearch(opensearch): TenantSearch,
    TenantEvents(events): TenantEvents,
    Path(scope): Path<DocumentBoxScope>,
    Garde(Json(req)): Garde<Json<CreateLink>>,
) -> Result<(StatusCode, Json<LinkWithExtra>), DynHttpError> {
    let folder = Folder::find_by_id(&db, &scope, req.folder_id)
        .await
        .context("unable to query folder")?
        .ok_or(HttpFolderError::UnknownFolder)?;

    // Update stored editing user data
    let created_by = action_user.store_user(&db).await?;

    let mut create_state = CreateLinkState::default();

    let create = CreateLinkData {
        folder,
        name: req.name,
        value: req.value,
        created_by: created_by.as_ref().map(|value| value.id.to_string()),
    };

    let link = match create_link(&db, &opensearch, &events, create, &mut create_state).await {
        Ok(value) => value,
        Err(err) => {
            // Attempt to rollback any allocated resources in the background
            tokio::spawn(async move {
                rollback_create_link(&opensearch, create_state).await;
            });

            return Err(anyhow::Error::from(err).into());
        }
    };

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

/// GET /box/:scope/link/:link_id
///
/// Gets a link
pub async fn get(
    TenantDb(db): TenantDb,
    Path((scope, link_id)): Path<(DocumentBoxScope, LinkId)>,
) -> HttpResult<LinkWithExtra> {
    let link = Link::find_with_extra(&db, &scope, link_id)
        .await
        .context("failed to query link")?
        .ok_or(HttpLinkError::UnknownLink)?;

    Ok(Json(link))
}

/// GET /box/:scope/link/:link_id/metadata
///
/// Gets a link metadata
pub async fn get_metadata(
    TenantDb(db): TenantDb,
    Extension(website_service): Extension<Arc<WebsiteMetaService>>,
    Path((scope, link_id)): Path<(DocumentBoxScope, LinkId)>,
) -> HttpResult<LinkMetadataResponse> {
    let link = Link::find_with_extra(&db, &scope, link_id)
        .await
        .context("failed to query link")?
        .ok_or(HttpLinkError::UnknownLink)?;

    let resolved = website_service
        .resolve_website(&link.value)
        .await
        .map_err(HttpLinkError::FailedResolve)?;

    Ok(Json(LinkMetadataResponse {
        title: resolved.title,
        og_title: resolved.og_title,
        og_description: resolved.og_description,
        favicon: resolved.favicon.is_some(),
        image: resolved.og_image.is_some(),
    }))
}

/// GET /box/:scope/link/:link_id/favicon
///
/// Gets a link website favicon
pub async fn get_favicon(
    TenantDb(db): TenantDb,
    Extension(website_service): Extension<Arc<WebsiteMetaService>>,
    Path((scope, link_id)): Path<(DocumentBoxScope, LinkId)>,
) -> Result<Response<Body>, DynHttpError> {
    let link = Link::find_with_extra(&db, &scope, link_id)
        .await
        .context("failed to query link")?
        .ok_or(HttpLinkError::UnknownLink)?;

    let resolved = website_service.resolve_website(&link.value).await?;
    let favicon = resolved.favicon.ok_or(HttpLinkError::NoFavicon)?;
    let body = axum::body::Body::from(favicon.bytes);

    Ok(Response::builder()
        .header(header::CONTENT_TYPE, favicon.content_type.to_string())
        .body(body)
        .context("failed to create response")?)
}

/// GET /box/:scope/link/:link_id/image
///
/// Gets a link website ogp image aka "Social Image"
pub async fn get_image(
    TenantDb(db): TenantDb,
    Extension(website_service): Extension<Arc<WebsiteMetaService>>,
    Path((scope, link_id)): Path<(DocumentBoxScope, LinkId)>,
) -> Result<Response<Body>, DynHttpError> {
    let link = Link::find_with_extra(&db, &scope, link_id)
        .await
        .context("failed to query link")?
        .ok_or(HttpLinkError::UnknownLink)?;

    let resolved = website_service.resolve_website(&link.value).await?;
    let og_image = resolved.og_image.ok_or(HttpLinkError::NoImage)?;
    let body = axum::body::Body::from(og_image.bytes);

    Ok(Response::builder()
        .header(header::CONTENT_TYPE, og_image.content_type.to_string())
        .body(body)
        .context("failed to create response")?)
}

/// GET /box/:scope/link/:link_id/edit-history
///
/// Gets the edit history for the provided link
pub async fn get_edit_history(
    TenantDb(db): TenantDb,
    Path((scope, link_id)): Path<(DocumentBoxScope, LinkId)>,
) -> HttpResult<Vec<EditHistory>> {
    _ = Link::find_with_extra(&db, &scope, link_id)
        .await
        .context("failed to query link")?
        .ok_or(HttpLinkError::UnknownLink)?;

    let history = EditHistory::all_by_link(&db, link_id)
        .await
        .context("failed to get link edit history")?;

    Ok(Json(history))
}

/// PUT /box/:scope/link/:link_id
///
/// Updates a link, can be a name change, value change, a folder move, or all
pub async fn update(
    action_user: ActionUser,
    TenantDb(db): TenantDb,
    TenantSearch(opensearch): TenantSearch,
    Path((scope, link_id)): Path<(DocumentBoxScope, LinkId)>,
    Garde(Json(req)): Garde<Json<UpdateLinkRequest>>,
) -> HttpStatusResult {
    let mut link = Link::find(&db, &scope, link_id)
        .await
        .context("failed to query link")?
        .ok_or(HttpLinkError::UnknownLink)?;

    let mut db = db.begin().await.context("failed to start transaction")?;

    // Update stored editing user data
    let user = action_user.store_user(db.deref_mut()).await?;
    let user_id = user.as_ref().map(|value| value.id.to_string());

    if let Some(target_id) = req.folder_id {
        // Ensure the target folder exists, also ensures the target folder is in the same scope
        // (We may allow across scopes in the future, but would need additional checks for access control of target scope)
        let target_folder = Folder::find_by_id(db.deref_mut(), &scope, target_id)
            .await
            .context("unknown target folder")?
            .ok_or(HttpFolderError::UnknownTargetFolder)?;

        link = move_link(&mut db, user_id.clone(), link, target_folder).await?;
    };

    if let Some(new_name) = req.name {
        link = update_link_name(&mut db, user_id.clone(), link, new_name).await?;
    }

    if let Some(new_value) = req.value {
        link = update_link_value(&mut db, user_id, link, new_value).await?;
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
        .context("failed to update search index for updated link")?;

    db.commit().await.context("failed to commit transaction")?;

    Ok(StatusCode::OK)
}

/// DELETE /box/:scope/link/:link_id
///
/// Deletes a link
pub async fn delete(
    TenantDb(db): TenantDb,
    TenantSearch(opensearch): TenantSearch,
    TenantEvents(events): TenantEvents,
    Path((scope, link_id)): Path<(DocumentBoxScope, LinkId)>,
) -> HttpStatusResult {
    let link = Link::find(&db, &scope, link_id)
        .await
        .context("failed to query file")?
        .ok_or(HttpLinkError::UnknownLink)?;

    delete_link(&db, &opensearch, &events, link, scope).await?;

    Ok(StatusCode::NO_CONTENT)
}
