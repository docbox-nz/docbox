use crate::error::HttpError;
use axum::http::StatusCode;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HttpTaskError {
    #[error("unknown task")]
    UnknownTask,

    #[error("internal server error")]
    Database,
}

impl HttpError for HttpTaskError {
    fn log(&self) {}

    fn status(&self) -> axum::http::StatusCode {
        match self {
            HttpTaskError::UnknownTask => StatusCode::NOT_FOUND,
            HttpTaskError::Database => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}
