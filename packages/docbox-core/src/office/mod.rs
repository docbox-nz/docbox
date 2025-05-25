use bytes::Bytes;
use convert_server::OfficeConverterServer;
use mime::Mime;
use office_convert_client::RequestError;
use thiserror::Error;

use crate::processing::pdf::is_pdf_file;

pub mod convert_server;

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

#[derive(Clone)]
pub enum OfficeConverter {
    ConverterServer(OfficeConverterServer),
}

impl OfficeConverter {
    pub async fn convert_to_pdf(&self, bytes: Bytes) -> Result<Bytes, PdfConvertError> {
        match self {
            OfficeConverter::ConverterServer(inner) => inner.convert_to_pdf(bytes).await,
        }
    }
}

/// Trait for converting some file input bytes into some output bytes
/// for a converted PDF file
pub(crate) trait ConvertToPdf {
    async fn convert_to_pdf(&self, bytes: Bytes) -> Result<Bytes, PdfConvertError>;
}

/// Checks if the provided mime type either is a PDF
/// or can be converted to a PDF
pub fn is_pdf_compatible(mime: &Mime) -> bool {
    // We don't want to send images through the office converter
    is_pdf_file(mime) || (mime.type_() != mime::IMAGE && is_known_convertable(mime.essence_str()))
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
