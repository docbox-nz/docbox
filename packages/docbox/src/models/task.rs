use crate::error::HttpError;
use axum::http::StatusCode;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HttpTaskError {
    #[error("unknown task")]
    UnknownTask,
}

impl HttpError for HttpTaskError {
    fn log(&self) {}

    fn status(&self) -> axum::http::StatusCode {
        match self {
            HttpTaskError::UnknownTask => StatusCode::NOT_FOUND,
        }
    }
}
