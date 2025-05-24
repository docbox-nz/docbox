use mime::Mime;

/// Supported upload file types
pub static ALLOWED_MIME_TYPES: &[&str] = &[
    // JSON and binary format, used internally
    "application/json",
    "application/octet-stream",
    // HTML and plain text formats
    "text/html",
    "text/plain",
    "text/spreadsheet",
    // Word Processing Documents
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
    "application/x-fictionbook+xml",
    "application/x-aportisdoc",
    "application/prs.plucker",
    "application/x-iwork-pages-sffpages",
    "application/vnd.palm",
    "application/epub+zip",
    "application/x-pocket-word",
    // Spreadsheets
    "application/vnd.oasis.opendocument.spreadsheet-flat-xml",
    "application/vnd.lotus-1-2-3",
    "application/vnd.ms-excel",
    "application/vnd.sun.xml.calc",
    "application/vnd.sun.xml.calc.template",
    "application/x-gnumeric",
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    "application/vnd.ms-excel.sheet.macroEnabled.12",
    "application/vnd.openxmlformats-officedocument.spreadsheetml.template",
    "application/x-iwork-numbers-sffnumbers",
    // Mathematical and Structured Documents
    "application/mathml+xml",
    "application/vnd.sun.xml.math",
    "application/vnd.oasis.opendocument.formula",
    "application/vnd.sun.xml.base",
    "application/docbook+xml",
    "application/xhtml+xml",
    // Presentations
    "application/vnd.ms-powerpoint",
    "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    "application/vnd.oasis.opendocument.presentation",
    // Images
    "image/jpeg",
    "image/gif",
    "image/bmp",
    "image/png",
    "image/svg+xml",
    "image/webp",
    // Zip files
    "application/zip",
    // Videos
    "video/mp4",
];

/// Mapping between mime types and their file extensions
#[rustfmt::skip]
pub static MIME_EXT_MAP: &[(&str, &str)] =&[
    // JSON and binary format
    ("application/json", "json"),
    ("application/octet-stream", "bin"),
    // HTML and plain text formats
    ("text/html", "html"),
    ("text/plain", "txt"),
    ("text/spreadsheet", "txt"),  // Rare, usually treated as CSV or TSV
    // Word Processing Documents
    ("application/msword", "doc"),
    ("application/vnd.oasis.opendocument.text-flat-xml", "fodt"),
    ("application/rtf", "rtf"),
    ("application/vnd.sun.xml.writer", "sxw"),
    ("application/vnd.wordperfect", "wpd"),
    ("application/vnd.ms-works", "wps"),
    ("application/x-mswrite", "wri"),
    ("application/clarisworks", "cwk"),
    ("application/macwriteii", "mw"),
    ("application/x-abiword", "abw"),
    ("application/x-t602", "602"),
    ("application/vnd.lotus-wordpro", "lwp"),
    ("application/x-hwp", "hwp"),
    ("application/vnd.sun.xml.writer.template", "stw"),
    ("application/pdf", "pdf"),
    ("application/vnd.oasis.opendocument.text", "odt"),
    ("application/vnd.oasis.opendocument.text-template", "ott"),
    ("application/vnd.openxmlformats-officedocument.wordprocessingml.document", "docx"),
    ("application/vnd.openxmlformats-officedocument.wordprocessingml.template", "dotx"),
    ("application/vnd.openxmlformats-officedocument.wordprocessingml.slideshow", "pptx"),  // Slideshow format
    ("application/x-fictionbook+xml", "fb2"),
    ("application/x-aportisdoc", "pdb"),
    ("application/prs.plucker", "pdb"),
    ("application/x-iwork-pages-sffpages", "pages"),
    ("application/vnd.palm", "pdb"),
    ("application/epub+zip", "epub"),
    ("application/x-pocket-word", "psw"),
    // Spreadsheets
    ("application/vnd.oasis.opendocument.spreadsheet-flat-xml", "fods"),
    ("application/vnd.lotus-1-2-3", "123"),
    ("application/vnd.ms-excel", "xls"),
    ("application/vnd.sun.xml.calc", "sxc"),
    ("application/vnd.sun.xml.calc.template", "stc"),
    ("application/x-gnumeric", "gnumeric"),
    ("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet", "xlsx"),
    ("application/vnd.ms-excel.sheet.macroEnabled.12", "xlsm"),
    ("application/vnd.openxmlformats-officedocument.spreadsheetml.template", "xltx"),
    ("application/x-iwork-numbers-sffnumbers", "numbers"),
    // Mathematical and Structured Documents
    ("application/mathml+xml", "mml"),
    ("application/vnd.sun.xml.math", "smf"),
    ("application/vnd.oasis.opendocument.formula", "odf"),
    ("application/vnd.sun.xml.base", "odb"),
    ("application/docbook+xml", "xml"),
    ("application/xhtml+xml", "xhtml"),
    // Presentations
    ("application/vnd.ms-powerpoint", "ppt"),
    ("application/vnd.openxmlformats-officedocument.presentationml.presentation", "pptx"),
    ("application/vnd.oasis.opendocument.presentation", "odp"),
    // Images
    ("image/jpeg", "jpg"),
    ("image/gif", "gif"),
    ("image/bmp", "bmp"),
    ("image/png", "png"),
    ("image/svg+xml", "svg"),
    ("image/webp", "webp"),
    // Zip files
    ("application/zip", "zip"),
    // Videos
    ("video/mp4", "mp4"),
];

/// Checks the provided mime fits into the list of allowed mime types
#[allow(unused)]
pub fn is_allowed_mime(mime: &Mime) -> bool {
    ALLOWED_MIME_TYPES.contains(&mime.essence_str())
}
