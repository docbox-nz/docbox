//! # Convert Server
//!
//! Persistent file conversion server https://github.com/jacobtread/office-convert-server backend
//! for performing office file conversion
//!
//! ## Environment Variables
//!
//! * `DOCBOX_CONVERT_SERVER_ADDRESS` - Comma separated list of server addresses
//! * `DOCBOX_CONVERT_SERVER_USE_PROXY` - Whether to use the system proxy when talking to the server

use crate::office::libreoffice::is_known_libreoffice_pdf_convertable;

use super::{ConvertToPdf, PdfConvertError};
use bytes::Bytes;
use office_convert_client::{
    OfficeConvertClient, OfficeConvertLoadBalancer, OfficeConverter, RequestError,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfficeConvertServerConfig {
    pub addresses: Vec<String>,
    pub use_proxy: bool,
}

impl OfficeConvertServerConfig {
    pub fn from_env() -> OfficeConvertServerConfig {
        let addresses = std::env::var("DOCBOX_CONVERT_SERVER_ADDRESS")
            .or(std::env::var("CONVERT_SERVER_ADDRESS"))
            .unwrap_or("http://127.0.0.1:8081".to_string());
        let addresses = addresses
            .split(',')
            .map(|value| value.to_string())
            .collect();

        // By default the office convert server will ignore the system proxy
        // since we don't want file conversion to take an extra network hop since
        // it shouldn't be leaving the private network
        //
        // CONVERT_SERVER_USE_PROXY allows this behavior to be disabled
        let use_proxy = match std::env::var("DOCBOX_CONVERT_SERVER_USE_PROXY")
            .or(std::env::var("CONVERT_SERVER_USE_PROXY"))
        {
            Ok(value) => match value.parse::<bool>() {
                Ok(value) => value,
                Err(error) => {
                    tracing::error!(
                        ?error,
                        "invalid CONVERT_SERVER_USE_PROXY environment variable, defaulting to false"
                    );
                    false
                }
            },
            Err(_) => false,
        };

        OfficeConvertServerConfig {
            addresses,
            use_proxy,
        }
    }
}

/// Variant of [ConvertToPdf] that uses LibreOffice through a
/// office-converter server for the conversion
#[derive(Clone)]
pub struct OfficeConverterServer {
    client: OfficeConverter,
}

#[derive(Debug, Error)]
pub enum OfficeConvertServerError {
    #[error("failed to build http client")]
    BuildHttpClient(reqwest::Error),
    #[error("no office convert server addresses provided")]
    NoAddresses,
}

impl OfficeConverterServer {
    pub fn new(client: OfficeConverter) -> Self {
        Self { client }
    }

    pub fn from_config(
        config: OfficeConvertServerConfig,
    ) -> Result<Self, OfficeConvertServerError> {
        Self::from_addresses(
            config.addresses.iter().map(|value| value.as_str()),
            config.use_proxy,
        )
    }

    pub fn from_addresses<'a, I>(
        addresses: I,
        use_proxy: bool,
    ) -> Result<Self, OfficeConvertServerError>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let mut convert_clients: Vec<OfficeConvertClient> = Vec::new();
        let mut http_client = Client::builder();

        if !use_proxy {
            http_client = http_client.no_proxy();
        }

        let http_client = http_client
            .build()
            .map_err(OfficeConvertServerError::BuildHttpClient)?;

        for convert_server_address in addresses {
            tracing::debug!(address = ?convert_server_address, "added convert server");

            let convert_client =
                OfficeConvertClient::from_client(convert_server_address, http_client.clone());

            convert_clients.push(convert_client);
        }

        if convert_clients.is_empty() {
            return Err(OfficeConvertServerError::NoAddresses);
        }

        // Create a convert load balancer
        let load_balancer = OfficeConvertLoadBalancer::new(convert_clients);
        Ok(Self::new(OfficeConverter::from_load_balancer(
            load_balancer,
        )))
    }
}

impl ConvertToPdf for OfficeConverterServer {
    async fn convert_to_pdf(&self, file_bytes: Bytes) -> Result<Bytes, PdfConvertError> {
        self.client
            .convert(file_bytes)
            .await
            .map_err(|err| match err {
                // File was encrypted
                RequestError::ErrorResponse { reason, .. } if reason == "file is encrypted" => {
                    PdfConvertError::EncryptedDocument
                }
                // File was corrupted or unreadable
                RequestError::ErrorResponse { reason, .. } if reason == "file is corrupted" => {
                    PdfConvertError::MalformedDocument
                }
                // Other unknown error
                err => PdfConvertError::ConversionFailed(err),
            })
    }

    fn is_convertable(&self, mime: &mime::Mime) -> bool {
        is_known_libreoffice_pdf_convertable(mime)
    }
}
