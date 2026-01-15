use axum::{
    extract::Request,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tower::{Layer, Service};

#[derive(Clone)]
pub struct ApiKeyLayer {
    key: String,
}

impl ApiKeyLayer {
    pub fn new(key: String) -> Self {
        Self { key }
    }
}

impl<S> Layer<S> for ApiKeyLayer {
    type Service = ApiKeyMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ApiKeyMiddleware {
            inner,
            key: self.key.clone(),
        }
    }
}

#[derive(Clone)]
pub struct ApiKeyMiddleware<S> {
    inner: S,
    key: String,
}

impl<S> Service<Request> for ApiKeyMiddleware<S>
where
    S: Service<Request, Response = Response> + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future =
        Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let header = match request.headers().get("x-docbox-api-key") {
            Some(value) => value,
            None => {
                return Box::pin(async move {
                    Ok((StatusCode::UNAUTHORIZED, "Missing x-docbox-api-key").into_response())
                });
            }
        };

        if header.to_str().is_ok_and(|value| value.ne(&self.key)) {
            return Box::pin(async move {
                Ok((
                    StatusCode::UNAUTHORIZED,
                    "Missing or invalid x-docbox-api-key",
                )
                    .into_response())
            });
        }

        Box::pin(self.inner.call(request))
    }
}
