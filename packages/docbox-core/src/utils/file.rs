use std::path::Path;

use mime::Mime;

// Set of characters to allow in S3 file names a-zA-Z0-9
static ALLOWED_S3_CHARS: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

pub fn make_s3_safe(name: &str) -> String {
    name.chars()
        .filter_map(|c| {
            if c.is_whitespace() || c == '-' {
                // Replace whitespace and dashes with underscores
                Some('_')
            } else if ALLOWED_S3_CHARS.contains(c) {
                // Allowed characters can stay
                Some(c)
            } else {
                // Ignore anything else
                None
            }
        })
        // Don't take more than 50 chars worth of name
        .take(50)
        .collect()
}

/// Extracts the extension portion of a file name
pub fn get_file_name_ext(name: &str) -> Option<String> {
    let path = Path::new(name);
    let ext = path.extension()?;
    let ext = ext.to_str()?;
    Some(ext.to_string())
}

/// Finds the file extension to use for a file based on its mime type
pub fn get_mime_ext(mime: &Mime) -> Option<&'static str> {
    MIME_EXT_MAP.iter().find_map(|value| {
        if value.0.eq(mime.essence_str()) {
            Some(value.1)
        } else {
            None
        }
    })
}

/// Mapping between mime types and their file extensions
#[rustfmt::skip]
pub static MIME_EXT_MAP: &[(&str, &str)] =&[
    ("application/vnd.ms-word.template.macroenabled.12", "dotm"),
    ("application/vnd.ms-excel.sheet.binary.macroenabled.12", "xlsb"),
    ("application/vnd.ms-excel.sheet.macroenabled.12", "xlsm"),
    ("application/vnd.ms-excel.template.macroenabled.12", "xltm"),
    ("application/vnd.ms-excel.template.macroenabled.12", "xltm"),
    ("application/vnd.oasis.opendocument.spreadsheet", "ods"),
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

#[cfg(test)]
mod test {
    use mime::Mime;

    use crate::utils::file::{get_file_name_ext, get_mime_ext, make_s3_safe};

    #[test]
    fn test_make_s3_safe_basic() {
        let input = "my file-name 123";
        let expected = "my_file_name_123";
        assert_eq!(make_s3_safe(input), expected);
    }

    #[test]
    fn test_make_s3_safe_only_allowed_chars() {
        let input = "abcXYZ0123";
        let expected = "abcXYZ0123";
        assert_eq!(make_s3_safe(input), expected);
    }

    #[test]
    fn test_make_s3_safe_removes_disallowed_chars() {
        let input = "file*name$with%chars!";
        let expected = "filenamewithchars";
        assert_eq!(make_s3_safe(input), expected);
    }

    #[test]
    fn test_make_s3_safe_max_length() {
        let input = "a".repeat(60); // 60 'a's
        let expected = "a".repeat(50); // only 50 allowed
        assert_eq!(make_s3_safe(&input), expected);
    }

    #[test]
    fn test_get_file_name_ext_basic() {
        let input = "file.txt";
        assert_eq!(get_file_name_ext(input), Some("txt".to_string()));
    }

    #[test]
    fn test_get_file_name_ext_no_ext() {
        let input = "file";
        assert_eq!(get_file_name_ext(input), None);
    }

    #[test]
    fn test_get_file_name_ext_hidden_file() {
        let input = ".hidden";
        assert_eq!(get_file_name_ext(input), None);
    }

    #[test]
    fn test_get_file_name_ext_multiple_dots() {
        let input = "archive.tar.gz";
        assert_eq!(get_file_name_ext(input), Some("gz".to_string()));
    }

    #[test]
    fn test_get_mime_ext_known_mime() {
        let mime: Mime = "image/png".parse().unwrap();
        assert_eq!(get_mime_ext(&mime), Some("png"));
    }

    #[test]
    fn test_get_mime_ext_unknown_mime() {
        let mime: Mime = "unknown/mime".parse().unwrap();
        assert_eq!(get_mime_ext(&mime), None);
    }
}
