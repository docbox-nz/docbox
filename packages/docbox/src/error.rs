use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use docbox_database::DbErr;
use serde::Serialize;
use std::{
    error::Error,
    fmt::{Debug, Display},
};
use thiserror::Error;
use tracing::error;
use utoipa::ToSchema;

/// Type alias for dynamic error handling and JSON responses
pub type HttpResult<T> = Result<Json<T>, DynHttpError>;

pub type HttpStatusResult = Result<StatusCode, DynHttpError>;

/// Wrapper for dynamic error handling using [HttpError] types
pub struct DynHttpError {
    /// The dynamic error cause
    inner: Box<dyn HttpError>,
}

impl Debug for DynHttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple(self.inner.type_name())
            .field(&self.inner)
            .finish()
    }
}

impl Display for DynHttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.inner, f)
    }
}

impl Error for DynHttpError {}

/// Handles converting the error into a response (Also logs the error before conversion)
impl IntoResponse for DynHttpError {
    fn into_response(self) -> Response {
        // Log the underlying error
        self.inner.log();

        // Create the response body
        let body = Json(HttpErrorResponse {
            reason: self.inner.reason(),
        });
        let status = self.inner.status();

        (status, body).into_response()
    }
}

/// Trait implemented by errors that can be converted into [HttpError]s
/// and used as error responses
pub trait HttpError: Error + Send + Sync + 'static {
    /// Handles how the error is logged, default implementation logs
    /// the [Display] and [Debug] variants
    fn log(&self) {
        error!("{self}: {self:?}");
    }

    /// Provides the HTTP [StatusCode] to use when creating this error response
    fn status(&self) -> StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }

    /// Provides the reason message to use in the error response
    fn reason(&self) -> String {
        self.to_string()
    }

    /// Provides the full type name for the actual error type thats been
    /// erased by dynamic typing (For better error source clarity)
    fn type_name(&self) -> &str {
        std::any::type_name::<Self>()
    }
}

/// Wrapper around [anyhow::Error] allowing it to be used as a [HttpError]
#[derive(Debug, Error)]
#[error(transparent)]
pub struct AnyhowHttpError(anyhow::Error);

impl HttpError for AnyhowHttpError {
    fn log(&self) {
        // Anyhow errors contain a stacktrace so only the debug variant is used
        error!("{:#?}", self.0);
    }

    fn reason(&self) -> String {
        // Anyhow errors use a generic message
        format!("{}", self.0)
    }
}

/// Allow conversion from anyhow errors into [DynHttpError] by wrapping
/// them with [AnyhowHttpError]
impl From<anyhow::Error> for DynHttpError {
    fn from(value: anyhow::Error) -> Self {
        DynHttpError {
            inner: Box::new(AnyhowHttpError(value)),
        }
    }
}

/// Wrapper around [DbErr] allowing it to be used as a [HttpError]
#[derive(Debug, Error)]
#[error("internal server error")]
pub struct DbHttpError(DbErr);

impl HttpError for DbHttpError {
    fn log(&self) {
        let cause = &self.0;
        error!(?cause, "db error in http response");
    }

    fn reason(&self) -> String {
        // Db errors use a generic message
        "internal server error".to_string()
    }
}

impl From<DbErr> for DynHttpError {
    fn from(value: DbErr) -> Self {
        DynHttpError {
            inner: Box::new(DbHttpError(value)),
        }
    }
}

/// Allow conversion from implementors of [HttpError] into a [DynHttpError]
impl<E> From<E> for DynHttpError
where
    E: HttpError,
{
    fn from(value: E) -> Self {
        DynHttpError {
            inner: Box::new(value),
        }
    }
}

/// HTTP error JSON format for serializing responses
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HttpErrorResponse {
    pub reason: String,
}
