use std::{
    error::Error,
    fmt::{Debug, Display},
};

#[derive(Debug, thiserror::Error)]
pub enum ManagementError {
    /// Error indicating the target service behind the management layer does
    /// not support the requested operation (i.e long running operation attempt on a serverless management runner)
    #[error("the target docbox service does not support the requested management operation")]
    UnsupportedOperation,

    /// Failed to serialize the response message from a dynamically handled command
    #[error("failed to serialize command response")]
    SerializeResponse(serde_json::Error),

    /// Any other service specific error
    #[error(transparent)]
    Service(#[from] DynServiceError),
}

pub trait DocboxServiceError: Error + Send + Sync + 'static {
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

pub struct DynServiceError {
    inner: Box<dyn DocboxServiceError>,
}

impl Debug for DynServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple(self.inner.type_name())
            .field(&self.inner)
            .finish()
    }
}

impl Display for DynServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.inner, f)
    }
}

impl Error for DynServiceError {
    fn cause(&self) -> Option<&dyn Error> {
        Some(self.inner.as_ref())
    }
}

impl<E> From<E> for DynServiceError
where
    E: DocboxServiceError,
{
    fn from(value: E) -> Self {
        DynServiceError {
            inner: Box::new(value),
        }
    }
}
