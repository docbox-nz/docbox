/// List of supported convertable formats
pub const LIBREOFFICE_CONVERTABLE_FORMATS: &[&str] = &[
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

/// Checks if the provided mime is included in the known convertable mime types
pub fn is_known_libreoffice_pdf_convertable(mime: &mime::Mime) -> bool {
    // We don't want to send images through the office converter
    mime.type_() != mime::IMAGE &&
    // Must be in the convertable formats list
    LIBREOFFICE_CONVERTABLE_FORMATS.contains(&mime.essence_str())
}
