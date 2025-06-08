use mime::Mime;
use std::path::Path;

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
    if let Some(known_match) = mime2ext::mime2ext(mime) {
        return Some(known_match);
    }

    // Search the fallback extension types
    OTHER_EXT_MAP.iter().find_map(|(other_mime, ext)| {
        if *other_mime == *mime {
            Some(*ext)
        } else {
            None
        }
    })
}

/// Fallback mapping for some more obscure mime types
/// 
/// Most of these are legacy types but supported by LibreOffice so
/// we support them here as well
#[rustfmt::skip]
pub static OTHER_EXT_MAP: &[(&str, &str)] = &[
    // Microsoft Excel Macro-Enabled Workbook
    ("application/vnd.ms-excel.sheet.macroEnabled.12", "xlsm"),
    // Flat OpenDocument Text file
    ("application/vnd.oasis.opendocument.text-flat-xml", "fodt"),
    // ClarisWorks document format (Legacy)
    ("application/clarisworks", "cwk"),
    // MacWrite II document format (Legacy)
    ("application/macwriteii", "mw"),
    // T602 word processor file (Legacy)
    ("application/x-t602", "602"),
    // Hangul Word Processor
    ("application/x-hwp", "hwp"),
    // FictionBook (FB2) e-book format
    ("application/x-fictionbook+xml", "fb2"),
    // AportisDoc eBook format (Legacy)
    ("application/x-aportisdoc", "pdb"),
    // Plucker eBook/Web content format (Legacy)
    ("application/prs.plucker", "pdb"),
    // Microsoft Pocket Word document (Legacy)
    ("application/x-pocket-word", "psw"),
    // Flat OpenDocument Spreadsheet
    ("application/vnd.oasis.opendocument.spreadsheet-flat-xml", "fods"),
    // OpenOffice Base files
    ("application/vnd.sun.xml.base", "odb"),

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
