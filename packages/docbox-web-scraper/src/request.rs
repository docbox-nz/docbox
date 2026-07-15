use crate::url_validation::UrlValidation;
use reqwest::{Response, StatusCode, header};
use thiserror::Error;
use url::Url;

const MAX_REDIRECT_ATTEMPTS: usize = 5;

#[derive(Debug, Error)]
pub enum RequestError {
    #[error("failed to request resource")]
    FailedRequest(reqwest::Error),

    #[error("error response from server")]
    ErrorResponse(reqwest::Error),

    #[error("disallowed target url")]
    DisallowedUrl,

    #[error("broken redirect")]
    BrokenRedirect,

    #[error("too many redirects")]
    TooManyRedirects,
}

pub async fn get_request<D: UrlValidation>(
    client: &reqwest::Client,
    url: Url,
) -> Result<(Response, usize), RequestError> {
    let mut redirect_attempts = MAX_REDIRECT_ATTEMPTS;
    let mut current_url = url;

    while redirect_attempts > 0 {
        let is_allowed = D::is_allowed_url(&current_url).await;
        if !is_allowed {
            tracing::warn!("skipping request for disallowed url: {current_url}");
            return Err(RequestError::DisallowedUrl);
        }

        let response = client
            .get(current_url.clone())
            .send()
            .await
            .map_err(RequestError::FailedRequest)?
            .error_for_status()
            .map_err(RequestError::ErrorResponse)?;

        if !matches!(
            response.status(),
            StatusCode::MOVED_PERMANENTLY
                | StatusCode::FOUND
                | StatusCode::SEE_OTHER
                | StatusCode::TEMPORARY_REDIRECT
                | StatusCode::PERMANENT_REDIRECT
        ) {
            return Ok((response, MAX_REDIRECT_ATTEMPTS - redirect_attempts));
        }

        let headers = response.headers();
        let location = headers
            .get(header::LOCATION)
            .ok_or_else(|| RequestError::BrokenRedirect)?;

        let location = location
            .to_str()
            .map_err(|_| RequestError::BrokenRedirect)?;

        // .join handles ./, ../, /, and event new URLs so all Location: cases are handled
        let next_url = current_url
            .join(location)
            .map_err(|_| RequestError::BrokenRedirect)?;

        current_url = next_url;
        redirect_attempts -= 1;
    }

    Err(RequestError::TooManyRedirects)
}

#[cfg(test)]
mod tests {
    use reqwest::{Client, redirect::Policy};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        task::AbortHandle,
    };

    use crate::url_validation::TokioDomainResolver;

    use super::*;

    struct AbortOnDrop(AbortHandle);

    impl Drop for AbortOnDrop {
        fn drop(&mut self) {
            self.0.abort();
        }
    }

    async fn mock_http_server(response: String) -> (Url, AbortOnDrop) {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("Failed to bind local server");
        let local_addr = listener.local_addr().expect("Failed to get local address");

        let handle = tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = [0; 1024];
                // Read incoming headers (we can ignore the contents for this test)
                let _ = socket.read(&mut buf).await;

                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.flush().await;
            }
        })
        .abort_handle();

        (
            Url::parse(&format!("http://{}", local_addr)).unwrap(),
            AbortOnDrop(handle),
        )
    }

    async fn spawn_redirect_server(next_url: String) -> (Url, AbortOnDrop) {
        let response = format!(
            "HTTP/1.1 307 Temporary Redirect\r\n\
                    Location: {next_url}\r\n\
                    Content-Length: 0\r\n\
                    Connection: close\r\n\
                    \r\n"
        );

        mock_http_server(response).await
    }

    struct MockUrlValidation;

    impl UrlValidation for MockUrlValidation {
        async fn is_allowed_url(url: &Url) -> bool {
            // We need at least one address locally that we can visit
            // in order to run the mock server
            if url.host_str().unwrap() == "127.0.0.1" {
                return true;
            }

            TokioDomainResolver::is_allowed_url(url).await
        }
    }

    /// Tests encountering redirects that reaches an allowed server
    #[tokio::test]
    async fn test_redirect_real() {
        let response = "HTTP/1.1 204 No Content\r\n\
                    Content-Length: 0\r\n\
                    Connection: close\r\n\
                    \r\n"
            .to_string();

        let (url_4, _handle) = mock_http_server(response).await;
        let (url_3, _handle) = spawn_redirect_server(url_4.to_string()).await;
        let (url_2, _handle) = spawn_redirect_server(url_3.to_string()).await;
        let (url_1, _handle) = spawn_redirect_server(url_2.to_string()).await;
        let client = Client::builder()
            .user_agent("DocboxLinkBot")
            .redirect(Policy::none())
            .build()
            .unwrap();

        let (_response, redirects) = get_request::<MockUrlValidation>(&client, url_1)
            .await
            .unwrap();

        assert_eq!(redirects, 3);
    }

    /// Tests encountering a bad redirect
    #[tokio::test]
    async fn test_redirect_real_disallowed_url() {
        let (url, _handle) = spawn_redirect_server("http://127.0.0.2".to_string()).await;
        let client = Client::builder()
            .user_agent("DocboxLinkBot")
            .redirect(Policy::none())
            .build()
            .unwrap();

        let error = get_request::<MockUrlValidation>(&client, url)
            .await
            .unwrap_err();

        assert!(matches!(error, RequestError::DisallowedUrl));
    }

    /// Tests following a chain of two good redirects leading to a third bad redirect
    /// which should be blocked
    #[tokio::test]
    async fn test_redirect_real_disallowed_url_chain() {
        let (url_3, _handle) = spawn_redirect_server("http://127.0.0.2".to_string()).await;
        let (url_2, _handle) = spawn_redirect_server(url_3.to_string()).await;
        let (url_1, _handle) = spawn_redirect_server(url_2.to_string()).await;

        let client = Client::builder()
            .user_agent("DocboxLinkBot")
            .redirect(Policy::none())
            .build()
            .unwrap();

        let error = get_request::<MockUrlValidation>(&client, url_1)
            .await
            .unwrap_err();

        assert!(matches!(error, RequestError::DisallowedUrl));
    }
}
