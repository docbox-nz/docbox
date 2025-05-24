//! File related endpoints

use std::{ops::DerefMut, str::FromStr};

use crate::{
    error::{DynHttpError, HttpResult, HttpStatusResult},
    middleware::{
        action_user::ActionUser,
        tenant::{TenantDb, TenantEvents, TenantSearch, TenantStorage},
    },
    models::{
        file::{
            CreatePresignedRequest, FileResponse, FileUploadResponse, HttpFileError,
            PresignedStatusResponse, PresignedUploadResponse, RawFileQuery, UpdateFileRequest,
            UploadFileRequest, UploadTaskResponse, UploadedFile,
        },
        folder::HttpFolderError,
    },
};
use anyhow::{anyhow, Context};
use axum::{
    body::Body,
    extract::{Path, Query},
    http::{header, HeaderValue, Response, StatusCode},
    Extension, Json,
};
use axum_typed_multipart::TypedMultipart;
use axum_valid::Garde;
use docbox_core::{
    processing::ProcessingLayer,
    search::models::UpdateSearchIndexData,
    services::files::{
        delete_file, move_file,
        presigned::{create_presigned_upload, CreatePresigned},
        update_file_name,
        upload::{safe_upload_file, ProcessingConfig, UploadFile, UploadedFileData},
    },
};
use docbox_database::models::{
    document_box::DocumentBoxScope,
    edit_history::EditHistory,
    file::{self, File, FileId, FileWithExtra},
    folder::Folder,
    generated_file::{GeneratedFile, GeneratedFileType},
    presigned_upload_task::{PresignedTaskStatus, PresignedUploadTask, PresignedUploadTaskId},
    tasks::{background_task, TaskStatus},
    user::User,
};
use mime::Mime;

/// POST /box/:scope/file
///
/// Uploads a new document to the provided document box folder
#[allow(clippy::too_many_arguments)]
pub async fn upload(
    action_user: ActionUser,
    TenantDb(db): TenantDb,
    TenantSearch(opensearch): TenantSearch,
    TenantStorage(s3): TenantStorage,
    TenantEvents(events): TenantEvents,
    Extension(processing): Extension<ProcessingLayer>,
    Path(scope): Path<DocumentBoxScope>,
    Garde(TypedMultipart(req)): Garde<TypedMultipart<UploadFileRequest>>,
) -> HttpResult<FileUploadResponse> {
    let folder = Folder::find_by_id(&db, &scope, req.folder_id)
        .await
        .context("unable to query folder")?
        .ok_or(HttpFolderError::UnknownTargetFolder)?;

    if let Some(fixed_id) = req.fixed_id {
        if File::find(&db, &scope, fixed_id)
            .await
            .context("failed to check duplicate files")?
            .is_some()
        {
            return Err(anyhow!("fixed file id already in use").into());
        }
    }

    let content_type = req
        .file
        .metadata
        .content_type
        .context("request file missing content type")?;

    let mime = Mime::from_str(&content_type).context("failed to parse content type")?;

    // Parse task processing config
    let processing_config: Option<ProcessingConfig> = match &req.processing_config {
        Some(value) => match serde_json::from_str(value) {
            Ok(value) => value,
            Err(cause) => {
                tracing::error!(?cause, "failed to deserialize processing config");
                None
            }
        },
        None => None,
    };

    // Update stored editing user data
    let created_by = action_user.store_user(&db).await?;

    // Create the upload configuration
    let upload = UploadFile {
        fixed_id: req.fixed_id,
        parent_id: req.parent_id,
        folder_id: folder.id,
        document_box: folder.document_box.clone(),
        name: req.name,
        mime,
        file_bytes: req.file.contents,
        created_by: created_by.as_ref().map(|value| value.id.to_string()),
        file_key: None,
        processing_config,
    };

    // Handle synchronous request waiting for the task to complete before responding
    if !req.asynchronous.unwrap_or_default() {
        let data = safe_upload_file(db, opensearch, s3, events, processing, upload).await?;
        let result = map_uploaded_file(data, &created_by);
        return Ok(Json(FileUploadResponse::Sync(Box::new(result))));
    }

    // Spawn background task
    let (task_id, created_at) = background_task(db.clone(), scope.clone(), async move {
        let result = safe_upload_file(db, opensearch, s3, events, processing, upload)
            .await
            // Map the response into the desired format
            .map(|data| map_uploaded_file(data, &created_by))
            // Serialize the response for storage
            .and_then(|value| serde_json::to_value(&value).context("failed to serialize output"));

        match result {
            Ok(value) => (TaskStatus::Completed, value),
            Err(err) => (
                TaskStatus::Failed,
                serde_json::json!({ "error": err.to_string() }),
            ),
        }
    })
    .await?;

    Ok(Json(FileUploadResponse::Async(UploadTaskResponse {
        task_id,
        created_at,
    })))
}

/// Map a [UploadedFileData] output from the core layer into the [UploadedFile]
/// HTTP response format
fn map_uploaded_file(data: UploadedFileData, created_by: &Option<User>) -> UploadedFile {
    let UploadedFileData {
        file,
        generated,
        additional_files,
    } = data;

    UploadedFile {
        file: FileWithExtra {
            id: file.id,
            name: file.name,
            mime: file.mime,
            folder_id: file.folder_id,
            hash: file.hash,
            size: file.size,
            encrypted: file.encrypted,
            created_at: file.created_at,
            created_by: file::CreatedByUser(created_by.clone()),
            last_modified_at: None,
            last_modified_by: file::LastModifiedByUser(None),
            parent_id: file.parent_id,
        },
        generated,

        // Map created file children
        additional_files: additional_files
            .into_iter()
            .map(|data| map_uploaded_file(data, created_by))
            .collect(),
    }
}

/// POST /box/:scope/file/presigned
///
/// Creates a new "presigned" upload, where the file is uploaded
/// directly to S3 [complete_presigned] is called by the client
/// after it has completed its upload
pub async fn create_presigned(
    action_user: ActionUser,
    TenantDb(db): TenantDb,
    TenantStorage(s3): TenantStorage,
    Path(scope): Path<DocumentBoxScope>,
    Garde(Json(req)): Garde<Json<CreatePresignedRequest>>,
) -> Result<(StatusCode, Json<PresignedUploadResponse>), DynHttpError> {
    let folder = Folder::find_by_id(&db, &scope, req.folder_id)
        .await
        .context("unable to query folder")?
        .ok_or(HttpFolderError::UnknownTargetFolder)?;

    // Update stored editing user data
    let created_by = action_user.store_user(&db).await?;

    let response = create_presigned_upload(
        &db,
        &s3,
        CreatePresigned {
            name: req.name,
            document_box: scope,
            folder,
            size: req.size,
            mime: req.mime,
            created_by: created_by.map(|user| user.id),
            parent_id: req.parent_id,
            processing_config: req.processing_config,
        },
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(PresignedUploadResponse {
            task_id: response.task_id,
            method: response.method,
            uri: response.uri,
            headers: response.headers,
        }),
    ))
}

/// GET /box/:scope/file/presigned/:task_id
///
/// Gets the current state of a presigned upload either pending or
/// complete, when complete the uploaded file and generated files
/// are returned
pub async fn get_presigned(
    TenantDb(db): TenantDb,
    Path((scope, task_id)): Path<(DocumentBoxScope, PresignedUploadTaskId)>,
) -> HttpResult<PresignedStatusResponse> {
    let task = PresignedUploadTask::find(&db, &scope, task_id)
        .await
        .context("unable to query presigned upload")?
        .ok_or(HttpFileError::UnknownTask)?;

    let file_id = match task.status {
        PresignedTaskStatus::Pending => return Ok(Json(PresignedStatusResponse::Pending)),
        PresignedTaskStatus::Completed { file_id } => file_id,
        PresignedTaskStatus::Failed { error } => {
            return Ok(Json(PresignedStatusResponse::Failed { error }))
        }
    };

    let file = File::find_with_extra(&db, &scope, file_id)
        .await
        .context("failed to query file")?
        .ok_or(HttpFileError::UnknownFile)?;

    let generated = GeneratedFile::find_all(&db, file_id)
        .await
        .context("query generated files")?;

    Ok(Json(PresignedStatusResponse::Complete { file, generated }))
}

/// GET /box/:scope/file/:file_id
///
/// Gets a specific file details, metadata and associated
/// generated files
pub async fn get(
    TenantDb(db): TenantDb,
    Path((scope, file_id)): Path<(DocumentBoxScope, FileId)>,
) -> HttpResult<FileResponse> {
    let file = File::find_with_extra(&db, &scope, file_id)
        .await
        .context("failed to query file")?
        .ok_or(HttpFileError::UnknownFile)?;

    let generated = GeneratedFile::find_all(&db, file_id)
        .await
        .context("query generated files")?;

    Ok(Json(FileResponse { file, generated }))
}

/// GET /box/:scope/file/:file_id/children
///
/// Get all children for the provided file
pub async fn get_children(
    TenantDb(db): TenantDb,
    Path((scope, file_id)): Path<(DocumentBoxScope, FileId)>,
) -> HttpResult<Vec<FileWithExtra>> {
    // Request the file first to ensure scoping rules
    _ = File::find_with_extra(&db, &scope, file_id)
        .await
        .context("failed to query file")?
        .ok_or(HttpFileError::UnknownFile)?;

    let files = File::find_by_parent_file_with_extra(&db, file_id)
        .await
        .context("failed to query file")?;

    Ok(Json(files))
}

/// GET /box/:scope/file/:file_id/edit-history
///
/// Gets the edit history for the provided file
pub async fn get_edit_history(
    TenantDb(db): TenantDb,
    Path((scope, file_id)): Path<(DocumentBoxScope, FileId)>,
) -> HttpResult<Vec<EditHistory>> {
    _ = File::find(&db, &scope, file_id)
        .await
        .context("failed to query file")?
        .ok_or(HttpFileError::UnknownFile)?;

    let edit_history = EditHistory::all_by_file(&db, file_id)
        .await
        .context("failed to get file history")?;

    Ok(Json(edit_history))
}

/// PUT /box/:scope/file/:file_id
///
/// Updates a file, can be a name change, a folder move, or both
pub async fn update(
    action_user: ActionUser,
    TenantDb(db): TenantDb,
    TenantSearch(opensearch): TenantSearch,
    Path((scope, file_id)): Path<(DocumentBoxScope, FileId)>,
    Garde(Json(req)): Garde<Json<UpdateFileRequest>>,
) -> HttpStatusResult {
    let mut file = File::find(&db, &scope, file_id)
        .await
        .context("failed to query file")?
        .ok_or(HttpFileError::UnknownFile)?;

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

        file = move_file(&mut db, user_id.clone(), file, target_folder).await?;
    };

    if let Some(new_name) = req.name {
        file = update_file_name(&mut db, user_id, file, new_name).await?;
    }

    // Update search index data
    opensearch
        .update_data(
            file.id,
            UpdateSearchIndexData {
                folder_id: Some(file.folder_id),
                name: Some(file.name.clone()),
                // Don't update unchanged
                mime: None,
                content: None,
                created_at: None,
                created_by: None,
                document_box: None,
                pages: None,
            },
        )
        .await
        .context("failed to update search index for new file name")?;

    db.commit().await.context("failed to commit transaction")?;

    Ok(StatusCode::OK)
}

/// GET /box/:scope/file/:file_id/raw
///
/// Gets a specific file contents unprocessed
pub async fn get_raw(
    TenantDb(db): TenantDb,
    TenantStorage(s3): TenantStorage,
    Path((scope, file_id)): Path<(DocumentBoxScope, FileId)>,
    Query(query): Query<RawFileQuery>,
) -> Result<Response<Body>, DynHttpError> {
    let file = File::find(&db, &scope, file_id)
        .await
        .context("failed to query file")?
        .ok_or(HttpFileError::UnknownFile)?;

    let byte_stream = s3
        .get_file(&file.file_key)
        .await
        .context("failed to get file from s3")?;

    let body = axum::body::Body::from_stream(byte_stream);

    let ty = if query.download {
        "attachment"
    } else {
        "inline"
    };

    let disposition = format!("{};filename=\"{}\"", ty, file.name);

    Ok(Response::builder()
        .header(header::CONTENT_TYPE, file.mime)
        .header(
            header::CONTENT_DISPOSITION,
            HeaderValue::from_str(&disposition).context("failed to create content disposition")?,
        )
        .body(body)
        .context("failed to create response")?)
}

/// DELETE /box/:scope/file/:file_id
///
/// Deletes the provided file
pub async fn delete(
    TenantDb(db): TenantDb,
    TenantStorage(storage): TenantStorage,
    TenantSearch(opensearch): TenantSearch,
    TenantEvents(events): TenantEvents,
    Path((scope, file_id)): Path<(DocumentBoxScope, FileId)>,
) -> HttpStatusResult {
    let file = File::find(&db, &scope, file_id)
        .await
        .context("failed to query file")?
        .ok_or(HttpFileError::UnknownFile)?;

    delete_file(&db, &storage, &opensearch, &events, file, scope)
        .await
        .context("failed to delete file")?;

    Ok(StatusCode::NO_CONTENT)
}

/// GET /box/:scope/file/:file_id/generated/:type
///
/// Request a generated file type for a file, gives back
/// metadata
pub async fn get_generated(
    TenantDb(db): TenantDb,
    Path((scope, file_id, generated_type)): Path<(DocumentBoxScope, FileId, GeneratedFileType)>,
) -> HttpResult<GeneratedFile> {
    let file = GeneratedFile::find(&db, &scope, file_id, generated_type)
        .await
        .context("failed to query file")?
        .ok_or(HttpFileError::NoMatchingGenerated)?;

    Ok(Json(file))
}

/// GET /box/:scope/file/:file_id/generated/:type/raw
///
/// Request the contents of a generated file type for a file
pub async fn get_generated_raw(
    TenantDb(db): TenantDb,
    TenantStorage(s3): TenantStorage,
    Path((scope, file_id, generated_type)): Path<(DocumentBoxScope, FileId, GeneratedFileType)>,
) -> Result<Response<Body>, DynHttpError> {
    let file = GeneratedFile::find(&db, &scope, file_id, generated_type)
        .await
        .context("failed to query file")?
        .ok_or(HttpFileError::NoMatchingGenerated)?;

    let byte_stream = s3
        .get_file(&file.file_key)
        .await
        .context("failed to get file from s3")?;

    let body = axum::body::Body::from_stream(byte_stream);

    Ok(Response::builder()
        .header(header::CONTENT_TYPE, file.mime)
        .header(
            header::CONTENT_DISPOSITION,
            HeaderValue::from_str("inline;filename=\"preview.pdf\"")
                .context("failed to create content disposition")?,
        )
        .body(body)
        .context("failed to create response")?)
}
