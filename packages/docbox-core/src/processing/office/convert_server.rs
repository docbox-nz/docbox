use bytes::Bytes;
use office_convert_client::{
    OfficeConvertClient, OfficeConvertLoadBalancer, OfficeConverter, RequestError,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{ConvertToPdf, PdfConvertError};

/// List of supported convertable formats
pub const CONVERTABLE_FORMATS: &[&str] = &[
    // .dotm
    "application/vnd.ms-word.template.macroenabled.12",
    // .xlsb
    "application/vnd.ms-excel.sheet.binary.macroenabled.12",
    // .xlsm
    "application/vnd.ms-excel.sheet.macroenabled.12",
    // .xltm
    "application/vnd.ms-excel.template.macroenabled.12",
    // .ods
    "application/vnd.oasis.opendocument.spreadsheet",
    "text/html",
    "application/msword",
    "application/vnd.oasis.opendocument.text-flat-xml",
    "application/rtf",
    "application/vnd.sun.xml.writer",
    "application/vnd.wordperfect",
    "application/vnd.ms-works",
    "application/x-mswrite",
    "application/clarisworks",
    "application/macwriteii",
    "application/x-abiword",
    "application/x-t602",
    "application/vnd.lotus-wordpro",
    "application/x-hwp",
    "application/vnd.sun.xml.writer.template",
    "application/pdf",
    "application/vnd.oasis.opendocument.text",
    "application/vnd.oasis.opendocument.text-template",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.template",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.slideshow",
    "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    "application/vnd.oasis.opendocument.presentation",
    "application/x-fictionbook+xml",
    "application/x-aportisdoc",
    "application/prs.plucker",
    "application/x-iwork-pages-sffpages",
    "application/vnd.palm",
    "application/epub+zip",
    "application/x-pocket-word",
    "application/vnd.oasis.opendocument.spreadsheet-flat-xml",
    "application/vnd.lotus-1-2-3",
    "application/vnd.ms-excel",
    "text/spreadsheet",
    "application/vnd.sun.xml.calc",
    "application/vnd.sun.xml.calc.template",
    "application/x-gnumeric",
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    "application/vnd.ms-excel.sheet.macroEnabled.12",
    "application/vnd.openxmlformats-officedocument.spreadsheetml.template",
    "application/clarisworks",
    "application/x-iwork-numbers-sffnumbers",
    "application/mathml+xml",
    "application/vnd.sun.xml.math",
    "application/vnd.oasis.opendocument.formula",
    "application/vnd.sun.xml.base",
    "image/jpeg",
    "image/png",
    "image/svg+xml",
    "image/webp",
    "application/docbook+xml",
    "application/xhtml+xml",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfficeConvertServerConfig {
    pub addresses: Vec<String>,
}

impl OfficeConvertServerConfig {
    pub fn from_env() -> OfficeConvertServerConfig {
        let addresses =
            std::env::var("CONVERT_SERVER_ADDRESS").unwrap_or("http://127.0.0.1:8081".to_string());
        let addresses = addresses
            .split(',')
            .map(|value| value.to_string())
            .collect();

        OfficeConvertServerConfig { addresses }
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
        Self::from_addresses(config.addresses.iter().map(|value| value.as_str()))
    }

    pub fn from_addresses<'a, I>(addresses: I) -> Result<Self, OfficeConvertServerError>
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
        is_known_pdf_convertable(mime)
    }
}

/// Checks if the provided mime is included in the known convertable mime types
pub fn is_known_pdf_convertable(mime: &mime::Mime) -> bool {
    // We don't want to send images through the office converter
    mime.type_() != mime::IMAGE &&
    // Must be in the convertable formats list
    CONVERTABLE_FORMATS.contains(&mime.essence_str())
}
