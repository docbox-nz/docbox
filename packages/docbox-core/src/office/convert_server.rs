use anyhow::Context;
use bytes::Bytes;
use office_convert_client::{
    OfficeConvertClient, OfficeConvertLoadBalancer, OfficeConverter, RequestError,
};
use reqwest::Client;

use super::{ConvertToPdf, PdfConvertError};

/// Variant of [ConvertToPdf] that uses LibreOffice through a
/// office-converter server for the conversion
#[derive(Clone)]
pub struct OfficeConverterServer {
    client: OfficeConverter,
}

impl OfficeConverterServer {
    pub fn new(client: OfficeConverter) -> Self {
        Self { client }
    }

    pub fn from_addresses<'a, I>(addresses: I) -> anyhow::Result<Self>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let mut convert_clients: Vec<OfficeConvertClient> = Vec::new();

        // Create an HTTP client with no_proxy to disable the system proxy
        // so that it will only be request over localhost
        // (Otherwise we will attempt to access the convert server through the proxy which is not able to access it)
        let http_client = Client::builder()
            .no_proxy()
            .build()
            .context("failed to build convert http client")?;

        for convert_server_address in addresses {
            tracing::debug!(address = ?convert_server_address, "added convert server");

            let convert_client =
                OfficeConvertClient::from_client(convert_server_address, http_client.clone());

            convert_clients.push(convert_client);
        }

        if convert_clients.is_empty() {
            return Err(anyhow::anyhow!(
                "no office convert server addresses provided"
            ));
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
}
