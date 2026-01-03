use axum::{
    http::{header::InvalidHeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use std::{
    error::Error,
    fmt::{Debug, Display},
};
use thiserror::Error;
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

impl HttpError for axum::http::Error {
    fn status(&self) -> StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

impl HttpError for InvalidHeaderValue {
    fn status(&self) -> StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

/// HTTP error JSON format for serializing responses
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HttpErrorResponse {
    pub reason: String,
}

#[derive(Debug, Error)]
pub enum HttpCommonError {
    #[error("internal server error")]
    ServerError,
}

impl HttpError for HttpCommonError {
    fn status(&self) -> axum::http::StatusCode {
        match self {
            HttpCommonError::ServerError => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}
