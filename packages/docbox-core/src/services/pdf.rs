//! Business logic for PDF conversion

use anyhow::Context;
use async_trait::async_trait;
use bytes::Bytes;
use mime::Mime;
use office_convert_client::{
    ConvertOffice, OfficeConvertClient, OfficeConvertLoadBalancer, RequestError,
};
use reqwest::Client;
use thiserror::Error;
use tracing::debug;

/// Environment variable to use for the convert server address
const CONVERT_SERVER_ADDRESS_ENV: &str = "CONVERT_SERVER_ADDRESS";

const DEFAULT_CONVERT_SERVER_ADDRESS: &str = "http://localhost:8081";

#[derive(Debug, Error)]
pub enum PdfConvertError {
    /// Failed to convert the file to a pdf
    #[error(transparent)]
    ConversionFailed(#[from] RequestError),

    #[error("office document is malformed")]
    MalformedDocument,

    #[error("office document is password protected")]
    EncryptedDocument,
}

/// Trait for converting some file input bytes into some output bytes
/// for a converted PDF file
#[async_trait]
pub(crate) trait ConvertToPdf {
    async fn convert_to_pdf(&self, bytes: Bytes) -> Result<Bytes, PdfConvertError>;
}

/// Variant of [ConvertToPdf] that uses LibreOffice for the conversion
#[derive(Clone)]
pub struct LibreOfficeConverter {
    pub converter: OfficeConvertLoadBalancer,
}

impl LibreOfficeConverter {
    pub fn init() -> anyhow::Result<Self> {
        // Determine the socket address to bind against
        let convert_server_addresses = std::env::var(CONVERT_SERVER_ADDRESS_ENV)
            .unwrap_or(DEFAULT_CONVERT_SERVER_ADDRESS.to_string());

        let mut convert_clients: Vec<OfficeConvertClient> = Vec::new();

        for convert_server_address in convert_server_addresses.split(',') {
            debug!(address = ?convert_server_address, "added convert server");

            let convert_client = OfficeConvertClient::from_client(
                convert_server_address,
                Client::builder()
                    .no_proxy()
                    .build()
                    .context("failed to build convert http client")?,
            )
            .context("failed to create converter client")?;
            convert_clients.push(convert_client);
        }

        if convert_clients.is_empty() {
            return Err(anyhow::anyhow!(
                "CONVERT_SERVER_ADDRESS did not contain any convert server addresses"
            ));
        }

        // Create a convert load balancer
        let convert_load_balancer = OfficeConvertLoadBalancer::new(convert_clients);

        Ok(Self {
            converter: convert_load_balancer,
        })
    }
}

#[async_trait]
impl ConvertToPdf for LibreOfficeConverter {
    async fn convert_to_pdf(&self, file_bytes: Bytes) -> Result<Bytes, PdfConvertError> {
        // Convert file to a pdf
        let output_data = match self.converter.convert(file_bytes).await {
            Ok(value) => value,
            Err(err) => match &err {
                RequestError::ErrorResponse { reason, .. } => {
                    if reason == "file is encrypted" {
                        return Err(PdfConvertError::EncryptedDocument);
                    }

                    if reason == "file is corrupted" {
                        return Err(PdfConvertError::MalformedDocument);
                    }

                    return Err(PdfConvertError::ConversionFailed(err));
                }
                _ => return Err(PdfConvertError::ConversionFailed(err)),
            },
        };

        Ok(output_data)
    }
}

/// Checks if the provided mime type either is a PDF
/// or can be converted to a PDF
pub fn is_pdf_compatible(mime: &Mime) -> bool {
    // We don't want to send images through the office converter
    is_pdf_file(mime) || (mime.type_() != mime::IMAGE && is_known_convertable(mime.essence_str()))
}

#[inline]
pub fn is_pdf_file(mime: &Mime) -> bool {
    mime.eq(&mime::APPLICATION_PDF)
}

/// Checks if the provided mime is included in the known convertable mime types
pub fn is_known_convertable(mime: &str) -> bool {
    CONVERTABLE_FORMATS.contains(&mime)
}

/// List of supported convertable formats
pub const CONVERTABLE_FORMATS: &[&str] = &[
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
    "text/plain",
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
