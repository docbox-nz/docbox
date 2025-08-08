//! File related endpoints

use crate::{
    error::{DynHttpError, HttpCommonError, HttpErrorResponse, HttpResult, HttpStatusResult},
    extensions::max_file_size::MaxFileSizeBytes,
    middleware::{
        action_user::{ActionUser, UserParams},
        tenant::{TenantDb, TenantEvents, TenantParams, TenantSearch, TenantStorage},
    },
    models::{
        document_box::DocumentBoxScope,
        file::{
            CreatePresignedRequest, FileResponse, FileUploadResponse, GetPresignedRequest,
            HttpFileError, PresignedDownloadResponse, PresignedStatusResponse,
            PresignedUploadResponse, RawFileQuery, UpdateFileRequest, UploadFileRequest,
            UploadTaskResponse, UploadedFile,
        },
        folder::HttpFolderError,
    },
};
use axum::{
    Extension, Json,
    body::Body,
    extract::{Path, Query},
    http::{HeaderValue, Response, StatusCode, header},
};
use axum_typed_multipart::TypedMultipart;
use axum_valid::Garde;
use docbox_core::{
    files::{
        delete_file::delete_file,
        update_file::{UpdateFile, UpdateFileError},
        upload_file::{ProcessingConfig, UploadFile, UploadedFileData, safe_upload_file},
        upload_file_presigned::{CreatePresigned, create_presigned_upload},
    },
    processing::ProcessingLayer,
    utils::file::get_file_name_ext,
};
use docbox_database::models::{
    edit_history::EditHistory,
    file::{self, File, FileId, FileWithExtra},
    folder::Folder,
    generated_file::{GeneratedFile, GeneratedFileType},
    presigned_upload_task::{PresignedTaskStatus, PresignedUploadTask, PresignedUploadTaskId},
    tasks::{TaskStatus, background_task},
    user::User,
};
use docbox_search::models::{FileSearchRequest, FileSearchResultResponse};
use mime::Mime;
use std::{str::FromStr, time::Duration};
use tracing::Instrument;

pub const FILE_TAG: &str = "File";

/// Upload file
///
/// Uploads a new document to the provided document box folder.
///
/// If the asynchronous option is specified a task will be returned
/// otherwise the completed file upload will be returned directly
///
/// In a browser environment its recommend to use the async option to
/// prevent running into browser timeouts if the processing takes too long.
///
/// In a reverse proxy + browser situation prefer using the presigned file upload
/// endpoint otherwise browsers may timeout while your server transfers the file
///
/// Synchronous uploads return [UploadedFile]
/// Asynchronous uploads return [UploadTaskResponse]
#[utoipa::path(
    post,
    operation_id = "file_upload",
    tag = FILE_TAG,
    path = "/box/{scope}/file",
    responses(
        (status = 200, description = "Upload or task created successfully", body = FileUploadResponse),
        (status = 400, description = "Malformed or invalid request not meeting validation requirements", body = HttpErrorResponse),
        (status = 404, description = "Target folder could not be found", body = HttpErrorResponse),
        (status = 409, description = "Fixed ID is already in use", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    request_body(content = UploadFileRequest, description = "Multipart upload", content_type = "multipart/form-data"),
    params(
        ("scope" = DocumentBoxScope, Path, description = "Scope to create the file within"),
        TenantParams,
        UserParams
    )
)]
#[tracing::instrument(skip_all, fields(scope = %scope))]
#[allow(clippy::too_many_arguments)]
pub async fn upload(
    action_user: ActionUser,
    TenantDb(db): TenantDb,
    TenantSearch(search): TenantSearch,
    TenantStorage(storage): TenantStorage,
    TenantEvents(events): TenantEvents,
    //
    Extension(processing): Extension<ProcessingLayer>,
    //
    Path(DocumentBoxScope(scope)): Path<DocumentBoxScope>,
    Garde(TypedMultipart(req)): Garde<TypedMultipart<UploadFileRequest>>,
) -> HttpResult<FileUploadResponse> {
    let folder = Folder::find_by_id(&db, &scope, req.folder_id)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query folder");
            HttpCommonError::ServerError
        })?
        .ok_or(HttpFolderError::UnknownTargetFolder)?;

    if let Some(fixed_id) = req.fixed_id {
        if File::find(&db, &scope, fixed_id)
            .await
            .map_err(|cause| {
                tracing::error!(?cause, "failed to check for duplicate files");
                HttpCommonError::ServerError
            })?
            .is_some()
        {
            return Err(DynHttpError::from(HttpFileError::FileIdInUse));
        }
    }

    let content_type = req
        .mime
        .or(req.file.metadata.content_type)
        .ok_or(HttpFileError::MissingMimeType)?;

    let mut mime = Mime::from_str(&content_type).map_err(|_| HttpFileError::InvalidMimeType)?;

    // Attempt to guess the file mime type when application/octet-stream is specified
    // (Likely from old browsers)
    if mime == mime::APPLICATION_OCTET_STREAM
        && req.disable_mime_sniffing.is_none_or(|value| !value)
    {
        let guessed_mime = get_file_name_ext(&req.name).and_then(|ext| {
            let guesses = mime_guess::from_ext(&ext);
            guesses.first()
        });

        if let Some(guessed_mime) = guessed_mime {
            mime = guessed_mime
        }
    }

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
        let data = safe_upload_file(db, search, storage, events, processing, upload)
            .await
            .map_err(|cause| {
                tracing::error!(?cause, "failed to upload file");
                HttpCommonError::ServerError
            })?;
        let result = map_uploaded_file(data, &created_by);
        return Ok(Json(FileUploadResponse::Sync(Box::new(result))));
    }

    let span = tracing::Span::current();

    // Spawn background task
    let (task_id, created_at) = background_task(
        db.clone(),
        scope.clone(),
        async move {
            let result = safe_upload_file(db, search, storage, events, processing, upload)
                .await
                .map_err(|cause| {
                    tracing::error!(?cause, "failed to upload file");
                    DynHttpError::from(HttpCommonError::ServerError)
                })
                // Map the response into the desired format
                .map(|data| map_uploaded_file(data, &created_by))
                // Serialize the response for storage
                .and_then(|value| {
                    serde_json::to_value(&value).map_err(|cause| {
                        tracing::error!(?cause, "failed to serialize upload task outcome");
                        DynHttpError::from(HttpCommonError::ServerError)
                    })
                });

            match result {
                Ok(value) => (TaskStatus::Completed, value),
                Err(err) => (
                    TaskStatus::Failed,
                    serde_json::json!({ "error": err.to_string() }),
                ),
            }
        }
        // Ensure the logging span is passed onto the background task so that
        // logging context continues
        .instrument(span),
    )
    .await
    .map_err(|cause| {
        tracing::error!(?cause, "failed to create background task");
        HttpCommonError::ServerError
    })?;

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
            pinned: file.pinned,
        },
        generated,

        // Map created file children
        additional_files: additional_files
            .into_iter()
            .map(|data| map_uploaded_file(data, created_by))
            .collect(),
    }
}

/// Create presigned file upload
///
/// Creates a new "presigned" upload, where the file is uploaded
/// directly to S3 [complete_presigned] is called by the client
/// after it has completed its upload
#[utoipa::path(
    post,
    operation_id = "file_create_presigned",
    tag = FILE_TAG,
    path = "/box/{scope}/file/presigned",
    responses(
        (status = 201, description = "Created presigned upload successfully", body = PresignedUploadResponse),
        (status = 400, description = "Malformed or invalid request not meeting validation requirements", body = HttpErrorResponse),
        (status = 404, description = "Target folder could not be found", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(
        ("scope" = DocumentBoxScope, Path, description = "Scope to create the file within"),
        TenantParams,
        UserParams
    )
)]
#[tracing::instrument(skip_all, fields(scope = %scope, req = ?req))]
pub async fn create_presigned(
    action_user: ActionUser,
    Extension(MaxFileSizeBytes(max_file_size)): Extension<MaxFileSizeBytes>,
    TenantDb(db): TenantDb,
    TenantStorage(storage): TenantStorage,
    Path(DocumentBoxScope(scope)): Path<DocumentBoxScope>,
    Garde(Json(req)): Garde<Json<CreatePresignedRequest>>,
) -> Result<(StatusCode, Json<PresignedUploadResponse>), DynHttpError> {
    if req.size > max_file_size {
        return Err(HttpFileError::FileTooLarge(req.size, max_file_size).into());
    }

    let folder = Folder::find_by_id(&db, &scope, req.folder_id)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query folder");
            HttpCommonError::ServerError
        })?
        .ok_or(HttpFolderError::UnknownTargetFolder)?;

    // Update stored editing user data
    let created_by = action_user.store_user(&db).await?;

    let mut mime = req.mime;

    // Attempt to guess the file mime type when application/octet-stream is specified
    // (Likely from old browsers)
    if mime == mime::APPLICATION_OCTET_STREAM
        && req.disable_mime_sniffing.is_none_or(|value| !value)
    {
        let guessed_mime = get_file_name_ext(&req.name).and_then(|ext| {
            let guesses = mime_guess::from_ext(&ext);
            guesses.first()
        });

        if let Some(guessed_mime) = guessed_mime {
            mime = guessed_mime
        }
    }

    let response = create_presigned_upload(
        &db,
        &storage,
        CreatePresigned {
            name: req.name,
            document_box: scope,
            folder,
            size: req.size,
            mime,
            created_by: created_by.map(|user| user.id),
            parent_id: req.parent_id,
            processing_config: req.processing_config,
        },
    )
    .await
    .map_err(|cause| {
        tracing::error!(?cause, "failed to create presigned upload");
        HttpCommonError::ServerError
    })?;

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

/// Get presigned file upload
///
/// Gets the current state of a presigned upload either pending or
/// complete, when complete the uploaded file and generated files
/// are returned
#[utoipa::path(
    get,
    operation_id = "file_get_presigned",
    tag = FILE_TAG,
    path = "/box/{scope}/file/presigned/{task_id}",
    responses(
        (status = 200, description = "Obtained presigned upload successfully", body = PresignedStatusResponse),
        (status = 404, description = "Presigned upload not found", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(
        ("scope" = DocumentBoxScope, Path, description = "Scope the file resides within"),
        ("task_id" = Uuid, Path, description = "ID of the task to query"),
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(scope = %scope, task_id = %task_id))]
pub async fn get_presigned(
    TenantDb(db): TenantDb,
    Path((scope, task_id)): Path<(DocumentBoxScope, PresignedUploadTaskId)>,
) -> HttpResult<PresignedStatusResponse> {
    let DocumentBoxScope(scope) = scope;

    let task = PresignedUploadTask::find(&db, &scope, task_id)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query presigned upload");
            HttpCommonError::ServerError
        })?
        .ok_or(HttpFileError::UnknownTask)?;

    let file_id = match task.status {
        PresignedTaskStatus::Pending => return Ok(Json(PresignedStatusResponse::Pending)),
        PresignedTaskStatus::Completed { file_id } => file_id,
        PresignedTaskStatus::Failed { error } => {
            return Ok(Json(PresignedStatusResponse::Failed { error }));
        }
    };

    let file = File::find_with_extra(&db, &scope, file_id)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query file");
            HttpCommonError::ServerError
        })?
        .ok_or(HttpFileError::UnknownFile)?;

    let generated = GeneratedFile::find_all(&db, file_id)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query generated files");
            HttpCommonError::ServerError
        })?;

    Ok(Json(PresignedStatusResponse::Complete { file, generated }))
}

/// Get file by ID
///
/// Gets a specific file details, metadata and associated
/// generated files
#[utoipa::path(
    get,
    operation_id = "file_get",
    tag = FILE_TAG,
    path = "/box/{scope}/file/{file_id}",
    responses(
        (status = 200, description = "Obtained file successfully", body = FileResponse),
        (status = 404, description = "File not found", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(
        ("scope" = DocumentBoxScope, Path, description = "Scope the file resides within"),
        ("file_id" = Uuid, Path, description = "ID of the file to query"),
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(scope = %scope, file_id = %file_id))]
pub async fn get(
    TenantDb(db): TenantDb,
    Path((scope, file_id)): Path<(DocumentBoxScope, FileId)>,
) -> HttpResult<FileResponse> {
    let DocumentBoxScope(scope) = scope;
    let file = File::find_with_extra(&db, &scope, file_id)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query file");
            HttpCommonError::ServerError
        })?
        .ok_or(HttpFileError::UnknownFile)?;

    let generated = GeneratedFile::find_all(&db, file_id)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query generated files");
            HttpCommonError::ServerError
        })?;

    Ok(Json(FileResponse { file, generated }))
}

/// Get file children
///
/// Get all children for the provided file, this is things like
/// attachments for processed emails
#[utoipa::path(
    get,
    operation_id = "file_get_children",
    tag = FILE_TAG,
    path = "/box/{scope}/file/{file_id}/children",
    responses(
        (status = 200, description = "Obtained children successfully", body = [FileWithExtra]),
        (status = 404, description = "File not found", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(
        ("scope" = DocumentBoxScope, Path, description = "Scope the file resides within"),
        ("file_id" = Uuid, Path, description = "ID of the file to query"),
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(scope = %scope, file_id = %file_id))]
pub async fn get_children(
    TenantDb(db): TenantDb,
    Path((scope, file_id)): Path<(DocumentBoxScope, FileId)>,
) -> HttpResult<Vec<FileWithExtra>> {
    let DocumentBoxScope(scope) = scope;

    // Request the file first to ensure scoping rules
    _ = File::find_with_extra(&db, &scope, file_id)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query file");
            HttpCommonError::ServerError
        })?
        .ok_or(HttpFileError::UnknownFile)?;

    let files = File::find_by_parent_file_with_extra(&db, file_id)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query file children");
            HttpCommonError::ServerError
        })?;

    Ok(Json(files))
}

/// Get file edit history
///
/// Gets the edit history for the provided file
#[utoipa::path(
    get,
    operation_id = "file_edit_history",
    tag = FILE_TAG,
    path = "/box/{scope}/file/{file_id}/edit-history",
    responses(
        (status = 200, description = "Obtained edit-history successfully", body = [EditHistory]),
        (status = 404, description = "File not found", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(
        ("scope" = DocumentBoxScope, Path, description = "Scope the file resides within"),
        ("file_id" = Uuid, Path, description = "ID of the file to query"),
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(scope = %scope, file_id = %file_id))]
pub async fn get_edit_history(
    TenantDb(db): TenantDb,
    Path((scope, file_id)): Path<(DocumentBoxScope, FileId)>,
) -> HttpResult<Vec<EditHistory>> {
    let DocumentBoxScope(scope) = scope;

    _ = File::find(&db, &scope, file_id)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query file");
            HttpCommonError::ServerError
        })?
        .ok_or(HttpFileError::UnknownFile)?;

    let edit_history = EditHistory::all_by_file(&db, file_id)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query file history");
            HttpCommonError::ServerError
        })?;

    Ok(Json(edit_history))
}

/// Update file
///
/// Updates a file, can be a name change, a folder move, or both
#[utoipa::path(
    put,
    operation_id = "file_update",
    tag = FILE_TAG,
    path = "/box/{scope}/file/{file_id}",
    responses(
        (status = 200, description = "Obtained edit-history successfully", body = [EditHistory]),
        (status = 404, description = "File not found", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(
        ("scope" = DocumentBoxScope, Path, description = "Scope the file resides within"),
        ("file_id" = Uuid, Path, description = "ID of the file to query"),
        TenantParams,
        UserParams
    )
)]
#[tracing::instrument(skip_all, fields(scope = %scope, file_id = %file_id, req = ?req))]
pub async fn update(
    action_user: ActionUser,
    TenantDb(db): TenantDb,
    TenantSearch(search): TenantSearch,
    Path((scope, file_id)): Path<(DocumentBoxScope, FileId)>,
    Garde(Json(req)): Garde<Json<UpdateFileRequest>>,
) -> HttpStatusResult {
    let DocumentBoxScope(scope) = scope;

    let file = File::find(&db, &scope, file_id)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query file");
            HttpCommonError::ServerError
        })?
        .ok_or(HttpFileError::UnknownFile)?;

    // Update stored editing user data
    let user = action_user.store_user(&db).await?;
    let user_id = user.as_ref().map(|value| value.id.to_string());

    let update = UpdateFile {
        folder_id: req.folder_id,
        name: req.name,
        pinned: req.pinned,
    };

    docbox_core::files::update_file::update_file(&db, &search, &scope, file, user_id, update)
        .await
        .map_err(|err| match err {
            UpdateFileError::UnknownTargetFolder => {
                DynHttpError::from(HttpFolderError::UnknownTargetFolder)
            }
            _ => DynHttpError::from(HttpCommonError::ServerError),
        })?;

    Ok(StatusCode::OK)
}

/// Get file raw
///
/// Requests the raw contents of a file, this is used for downloading
/// the file or viewing it in the browser or simply requesting its content
#[utoipa::path(
    get,
    operation_id = "file_get_raw",
    tag = FILE_TAG,
    path = "/box/{scope}/file/{file_id}/raw",
    responses(
        (status = 200, description = "Obtained raw file successfully"),
        (status = 404, description = "File not found", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(
        ("scope" = DocumentBoxScope, Path, description = "Scope the file resides within"),
        ("file_id" = Uuid, Path, description = "ID of the file to query"),
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(scope = %scope, file_id = %file_id, query = ?query))]
pub async fn get_raw(
    TenantDb(db): TenantDb,
    TenantStorage(storage): TenantStorage,
    Path((scope, file_id)): Path<(DocumentBoxScope, FileId)>,
    Query(query): Query<RawFileQuery>,
) -> Result<Response<Body>, DynHttpError> {
    let DocumentBoxScope(scope) = scope;

    let file = File::find(&db, &scope, file_id)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query file");
            HttpCommonError::ServerError
        })?
        .ok_or(HttpFileError::UnknownFile)?;

    let byte_stream = storage.get_file(&file.file_key).await.map_err(|cause| {
        tracing::error!(?cause, "failed to get file from S3");
        HttpCommonError::ServerError
    })?;

    let body = axum::body::Body::from_stream(byte_stream);

    let ty = if query.download {
        "attachment"
    } else {
        "inline"
    };

    let disposition = format!("{};filename=\"{}\"", ty, file.name);

    let csp = match mime::Mime::from_str(&file.mime) {
        // Images are served with a strict image only content security policy
        Ok(mime) if mime.type_() == mime::IMAGE => {
            "default-src 'none'; style-src 'self' 'unsafe-inline'; img-src 'self' data:;"
        }
        // Default policy
        _ => "script-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'",
    };

    Ok(Response::builder()
        .header(header::CONTENT_TYPE, file.mime)
        .header(header::CONTENT_SECURITY_POLICY, csp)
        .header(
            header::CONTENT_DISPOSITION,
            HeaderValue::from_str(&disposition)?,
        )
        .body(body)?)
}

/// Get file raw presigned
///
/// Requests the raw contents of a file as a presigned URL, used for
/// letting the client directly download a file from AWS instead of
/// downloading through the server
#[utoipa::path(
    post,
    operation_id = "file_get_raw_presigned",
    tag = FILE_TAG,
    path = "/box/{scope}/file/{file_id}/raw-presigned",
    responses(
        (status = 200, description = "Obtained raw file successfully"),
        (status = 404, description = "File not found", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(
        ("scope" = DocumentBoxScope, Path, description = "Scope the file resides within"),
        ("file_id" = Uuid, Path, description = "ID of the file to download"),
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(scope = %scope, file_id = %file_id, req = ?req))]
pub async fn get_raw_presigned(
    TenantDb(db): TenantDb,
    TenantStorage(storage): TenantStorage,
    Path((scope, file_id)): Path<(DocumentBoxScope, FileId)>,
    Json(req): Json<GetPresignedRequest>,
) -> HttpResult<PresignedDownloadResponse> {
    let DocumentBoxScope(scope) = scope;

    let file = File::find(&db, &scope, file_id)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query file");
            HttpCommonError::ServerError
        })?
        .ok_or(HttpFileError::UnknownFile)?;

    let expires_at = req.expires_at.unwrap_or(900);
    let expires_at = Duration::from_secs(expires_at as u64);

    let (signed_request, expires_at) = storage
        .create_presigned_download(&file.file_key, expires_at)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to created file presigned download");
            HttpCommonError::ServerError
        })?;

    Ok(Json(PresignedDownloadResponse {
        method: signed_request.method().to_string(),
        uri: signed_request.uri().to_string(),
        headers: signed_request
            .headers()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect(),
        expires_at,
    }))
}

/// Get file raw named
///
/// Requests the raw contents of a file, this is used for downloading
/// the file or viewing it in the browser or simply requesting its content
///
/// This is identical to [get_raw] except it takes an additional catch-all
/// tail parameter that's used to give a file name to the browser for things
/// like the in-browser PDF viewers. Browsers (Chrome) don't always listen to the
/// Content-Disposition file name so this is required
#[utoipa::path(
    get,
    operation_id = "file_get_raw_named",
    tag = FILE_TAG,
    path = "/box/{scope}/file/{file_id}/raw/{*file_name}",
    responses(
        (status = 200, description = "Obtained raw file successfully"),
        (status = 404, description = "File not found", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(
        ("scope" = DocumentBoxScope, Path, description = "Scope the file resides within"),
        ("file_id" = Uuid, Path, description = "ID of the file to query"),
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(scope = %scope, file_id = %file_id, query = ?query))]
pub async fn get_raw_named(
    db: TenantDb,
    storage: TenantStorage,
    Path((scope, file_id, _tail)): Path<(DocumentBoxScope, FileId, String)>,
    query: Query<RawFileQuery>,
) -> Result<Response<Body>, DynHttpError> {
    get_raw(db, storage, Path((scope, file_id)), query).await
}

/// Search
///
/// Search within the contents of the file
#[utoipa::path(
    post,
    operation_id = "file_search",
    tag = FILE_TAG,
    path = "/box/{scope}/file/{file_id}/search",
    responses(
        (status = 200, description = "Searched successfully", body = FileSearchResultResponse),
        (status = 400, description = "Malformed or invalid request not meeting validation requirements", body = HttpErrorResponse),
        (status = 404, description = "File not found", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(
        ("scope" = DocumentBoxScope, Path, description = "Scope the file resides within"),
        ("file_id" = Uuid, Path, description = "ID of the file to query"),
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(%scope, %file_id, ?req))]
pub async fn search(
    TenantDb(db): TenantDb,
    TenantSearch(search): TenantSearch,
    Path((scope, file_id)): Path<(DocumentBoxScope, FileId)>,
    Json(req): Json<FileSearchRequest>,
) -> HttpResult<FileSearchResultResponse> {
    let DocumentBoxScope(scope) = scope;

    // Assert the file exists
    _ = File::find(&db, &scope, file_id)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query file");
            HttpCommonError::ServerError
        })?
        .ok_or(HttpFileError::UnknownFile)?;

    let result = search
        .search_index_file(&scope, file_id, req)
        .await
        .map_err(|error| {
            tracing::error!(?error, "failed to search document box");
            HttpCommonError::ServerError
        })?;

    Ok(Json(FileSearchResultResponse {
        total_hits: result.total_hits,
        results: result.results,
    }))
}

/// Delete file by ID
///
/// Deletes the provided file
#[utoipa::path(
    delete,
    operation_id = "file_delete",
    tag = FILE_TAG,
    path = "/box/{scope}/file/{file_id}",
    responses(
        (status = 204, description = "Deleted file successfully"),
        (status = 404, description = "File not found", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(
        ("scope" = DocumentBoxScope, Path, description = "Scope the file resides within"),
        ("file_id" = Uuid, Path, description = "ID of the file to delete"),
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(scope = %scope, file_id = %file_id))]
pub async fn delete(
    TenantDb(db): TenantDb,
    TenantStorage(storage): TenantStorage,
    TenantSearch(search): TenantSearch,
    TenantEvents(events): TenantEvents,
    Path((scope, file_id)): Path<(DocumentBoxScope, FileId)>,
) -> HttpStatusResult {
    let DocumentBoxScope(scope) = scope;

    let file = File::find(&db, &scope, file_id)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query file");
            HttpCommonError::ServerError
        })?
        .ok_or(HttpFileError::UnknownFile)?;

    delete_file(&db, &storage, &search, &events, file, scope)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to delete file");
            HttpCommonError::ServerError
        })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Get generated file
///
/// Requests metadata about a specific generated file type for
/// a file, will return the details about the generated file
/// if it exists
#[utoipa::path(
    get,
    operation_id = "file_get_generated",
    tag = FILE_TAG,
    path = "/box/{scope}/file/{file_id}/generated/{type}",
    responses(
        (status = 200, description = "Obtained generated file successfully", body = GeneratedFile),
        (status = 404, description = "Generated file not found", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(
        ("scope" = DocumentBoxScope, Path, description = "Scope the file resides within"),
        ("file_id" = Uuid, Path, description = "ID of the file to query"),
        ("type" = GeneratedFileType, Path, description = "ID of the file to query"),
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(scope = %scope, file_id = %file_id, generated_type = %generated_type))]
pub async fn get_generated(
    TenantDb(db): TenantDb,
    Path((scope, file_id, generated_type)): Path<(DocumentBoxScope, FileId, GeneratedFileType)>,
) -> HttpResult<GeneratedFile> {
    let DocumentBoxScope(scope) = scope;

    let file = GeneratedFile::find(&db, &scope, file_id, generated_type)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query generated file");
            HttpCommonError::ServerError
        })?
        .ok_or(HttpFileError::NoMatchingGenerated)?;

    Ok(Json(file))
}

/// Get generated file raw
///
/// Request the contents of a specific generated file type
/// for a file, will return the file contents
#[utoipa::path(
    get,
    operation_id = "file_get_generated_raw",
    tag = FILE_TAG,
    path = "/box/{scope}/file/{file_id}/generated/{type}/raw",
    responses(
        (status = 200, description = "Obtained raw file successfully"),
        (status = 404, description = "Generated file not found", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(
        ("scope" = DocumentBoxScope, Path, description = "Scope the file resides within"),
        ("file_id" = Uuid, Path, description = "ID of the file to query"),
        ("type" = GeneratedFileType, Path, description = "ID of the file to query"),
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(scope = %scope, file_id = %file_id, generated_type = %generated_type))]
pub async fn get_generated_raw(
    TenantDb(db): TenantDb,
    TenantStorage(storage): TenantStorage,
    Path((scope, file_id, generated_type)): Path<(DocumentBoxScope, FileId, GeneratedFileType)>,
) -> Result<Response<Body>, DynHttpError> {
    let DocumentBoxScope(scope) = scope;

    let file = GeneratedFile::find(&db, &scope, file_id, generated_type)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query generated file");
            HttpCommonError::ServerError
        })?
        .ok_or(HttpFileError::NoMatchingGenerated)?;

    let byte_stream = storage.get_file(&file.file_key).await.map_err(|cause| {
        tracing::error!(?cause, "failed to file from S3");
        HttpCommonError::ServerError
    })?;

    let body = axum::body::Body::from_stream(byte_stream);

    let csp = match mime::Mime::from_str(&file.mime) {
        // Images are served with a strict image only content security policy
        Ok(mime) if mime.type_() == mime::IMAGE => "default-src 'none'; img-src 'self' data:;",
        // Default policy
        _ => "script-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'",
    };

    Ok(Response::builder()
        .header(header::CONTENT_TYPE, file.mime)
        .header(header::CONTENT_SECURITY_POLICY, csp)
        .header(
            header::CONTENT_DISPOSITION,
            HeaderValue::from_str("inline;filename=\"preview.pdf\"")?,
        )
        .body(body)?)
}

/// Get generated file raw presigned
///
/// Requests the raw contents of a generated file as a presigned URL,
/// used for letting the client directly download a file from AWS
/// instead of downloading through the server and gateway
#[utoipa::path(
    post,
    operation_id = "file_get_generated_raw_presigned",
    tag = FILE_TAG,
    path = "/box/{scope}/file/{file_id}/generated/{type}/raw-presigned",
    responses(
        (status = 200, description = "Obtained raw file successfully"),
        (status = 404, description = "Generated file not found", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(
        ("scope" = DocumentBoxScope, Path, description = "Scope the file resides within"),
        ("file_id" = Uuid, Path, description = "ID of the file to query"),
        ("type" = GeneratedFileType, Path, description = "ID of the file to query"),
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(scope = %scope, file_id = %file_id, generated_type = %generated_type, req = ?req))]
pub async fn get_generated_raw_presigned(
    TenantDb(db): TenantDb,
    TenantStorage(storage): TenantStorage,
    Path((scope, file_id, generated_type)): Path<(DocumentBoxScope, FileId, GeneratedFileType)>,
    Json(req): Json<GetPresignedRequest>,
) -> HttpResult<PresignedDownloadResponse> {
    let DocumentBoxScope(scope) = scope;

    let file = GeneratedFile::find(&db, &scope, file_id, generated_type)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to query generated file");
            HttpCommonError::ServerError
        })?
        .ok_or(HttpFileError::NoMatchingGenerated)?;

    let expires_at = req.expires_at.unwrap_or(900);
    let expires_at = Duration::from_secs(expires_at as u64);

    let (signed_request, expires_at) = storage
        .create_presigned_download(&file.file_key, expires_at)
        .await
        .map_err(|cause| {
            tracing::error!(?cause, "failed to created file presigned download");
            HttpCommonError::ServerError
        })?;

    Ok(Json(PresignedDownloadResponse {
        method: signed_request.method().to_string(),
        uri: signed_request.uri().to_string(),
        headers: signed_request
            .headers()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect(),
        expires_at,
    }))
}

/// Get generated file raw named
///
/// Request the contents of a specific generated file type
/// for a file, will return the file contents
///
/// See [get_raw_named] for reasoning
#[utoipa::path(
    get,
    operation_id = "file_get_generated_raw_named",
    tag = FILE_TAG,
    path = "/box/{scope}/file/{file_id}/generated/{type}/raw/{*tail}",
    responses(
        (status = 200, description = "Obtained raw file successfully"),
        (status = 404, description = "Generated file not found", body = HttpErrorResponse),
        (status = 500, description = "Internal server error", body = HttpErrorResponse)
    ),
    params(
        ("scope" = DocumentBoxScope, Path, description = "Scope the file resides within"),
        ("file_id" = Uuid, Path, description = "ID of the file to query"),
        ("type" = GeneratedFileType, Path, description = "ID of the file to query"),
        TenantParams
    )
)]
#[tracing::instrument(skip_all, fields(scope = %scope, file_id = %file_id, generated_type = %generated_type))]
pub async fn get_generated_raw_named(
    db: TenantDb,
    storage: TenantStorage,
    Path((scope, file_id, generated_type, _tail)): Path<(
        DocumentBoxScope,
        FileId,
        GeneratedFileType,
        String,
    )>,
) -> Result<Response<Body>, DynHttpError> {
    get_generated_raw(db, storage, Path((scope, file_id, generated_type))).await
}
