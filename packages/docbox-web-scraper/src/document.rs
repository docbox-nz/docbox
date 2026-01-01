//! # Document
//!
//! HTML document related logic for extracting information scraped from remote
//! HTML pages such as OGP metadata <title/> tags etc

use mime::Mime;
use std::str::FromStr;
use thiserror::Error;
use tl::{HTMLTag, Parser};
use url::Url;

/// Metadata extracted from a website
#[derive(Debug)]
pub struct WebsiteMetadata {
    pub title: Option<String>,
    pub og_title: Option<String>,
    pub og_description: Option<String>,
    pub og_image: Option<String>,
    pub favicons: Vec<Favicon>,
}

/// Favicon extracted from a website
#[derive(Debug, Clone)]
pub struct Favicon {
    /// Mime type for the favicon
    pub ty: Mime,
    /// Size if known
    pub sizes: Option<String>,
    /// URL of the favicon
    pub href: String,
}

/// State for data extracted from a website document
#[derive(Default)]
struct WebsiteDocumentState {
    title: Option<String>,
    description: Option<String>,
    og_title: Option<String>,
    og_description: Option<String>,
    og_image: Option<String>,
    favicons: Vec<Favicon>,
}

/// Errors that could occur when website metadata is loaded
#[derive(Debug, Error)]
pub enum WebsiteMetadataError {
    #[error("failed to request resource")]
    FailedRequest(reqwest::Error),

    #[error("error response from server")]
    ErrorResponse(reqwest::Error),

    #[error("failed to read response")]
    ReadResponse(reqwest::Error),

    #[error(transparent)]
    Parse(WebsiteMetadataParseError),
}

/// Errors that could occur when parsing the website metadata
#[derive(Debug, Error)]
pub enum WebsiteMetadataParseError {
    #[error("failed to parse resource response")]
    Parsing,
    #[error("failed to query page head")]
    QueryHead,
    #[error("page missing head element")]
    MissingHead,
    #[error("failed to parse head element")]
    InvalidHead,
    #[error("head element has no children")]
    EmptyHead,
}

/// Connects to a website reading the HTML contents, extracts the metadata
/// required from the <head/> element
pub async fn get_website_metadata(
    client: &reqwest::Client,
    url: &Url,
) -> Result<WebsiteMetadata, WebsiteMetadataError> {
    let mut url = url.clone();

    // Get the path from the URL
    let path = url.path();

    // Check if the path ends with a common HTML extension or if it is empty
    if !path.ends_with(".html") && !path.ends_with(".htm") && path.is_empty() {
        // Append /index.html if needed
        url.set_path("/index.html");
    }

    // Request page at URL
    let response = client
        .get(url)
        .send()
        .await
        .map_err(WebsiteMetadataError::FailedRequest)?
        .error_for_status()
        .map_err(WebsiteMetadataError::ErrorResponse)?;

    // Read response text
    let text = response
        .text()
        .await
        .map_err(WebsiteMetadataError::ReadResponse)?;

    parse_website_metadata(&text).map_err(WebsiteMetadataError::Parse)
}

/// Error's that can occur when attempting to load robots.txt
#[derive(Debug, Error)]
pub enum RobotsTxtError {
    #[error("failed to request resource")]
    FailedRequest(reqwest::Error),

    #[error("error response from server")]
    ErrorResponse(reqwest::Error),

    #[error("failed to read response")]
    ReadResponse(reqwest::Error),
}

/// Attempts to read the robots.txt file for the website to determine if
/// scraping is allowed
pub async fn is_allowed_robots_txt(
    client: &reqwest::Client,
    url: &Url,
) -> Result<bool, RobotsTxtError> {
    let mut url = url.clone();

    let original_url = url.to_string();

    // Change path to /robots.txt
    url.set_path("/robots.txt");

    // Request page at URL
    let response = client
        .get(url)
        .send()
        .await
        .map_err(RobotsTxtError::FailedRequest)?
        .error_for_status()
        .map_err(RobotsTxtError::ErrorResponse)?;

    // Read response text
    let robots_txt = response
        .text()
        .await
        .map_err(RobotsTxtError::ReadResponse)?;

    let mut matcher = robotstxt::DefaultMatcher::default();
    let is_allowed =
        matcher.one_agent_allowed_by_robots(&robots_txt, "DocboxLinkBot", &original_url);

    Ok(is_allowed)
}

/// Parse website metadata contained within the provided HTML content
pub fn parse_website_metadata(html: &str) -> Result<WebsiteMetadata, WebsiteMetadataParseError> {
    let dom = tl::parse(html, tl::ParserOptions::default())
        .map_err(|_| WebsiteMetadataParseError::Parsing)?;

    let parser = dom.parser();

    // Find the head element
    let head = dom
        .query_selector("head")
        .ok_or(WebsiteMetadataParseError::QueryHead)?
        .next()
        .ok_or(WebsiteMetadataParseError::MissingHead)?
        .get(parser)
        .ok_or(WebsiteMetadataParseError::InvalidHead)?;

    let mut state = WebsiteDocumentState::default();

    let children = head
        .children()
        .ok_or(WebsiteMetadataParseError::EmptyHead)?;
    for child in children.all(parser) {
        let tag = match child.as_tag() {
            Some(tag) => tag,
            None => continue,
        };

        match tag.name().as_bytes() {
            // Extract page title tag
            b"title" => visit_title_tag(&mut state, parser, tag),
            // Extract metadata
            b"meta" => visit_meta_tag(&mut state, tag),
            // Extract favicons
            b"link" => visit_link_tag(&mut state, tag),
            // Ignore other tags
            _ => {}
        }
    }

    // Fallback to description
    let og_description = state.og_description.or(state.description);

    Ok(WebsiteMetadata {
        title: state.title,
        og_title: state.og_title,
        og_description,
        og_image: state.og_image,
        favicons: state.favicons,
    })
}

/// Determines which favicon to use from the provided list
///
/// Prefers .ico format currently then defaulting to first
/// available. At a later date might want to check the sizes
/// field
pub fn determine_best_favicon(favicons: &[Favicon]) -> Option<&Favicon> {
    favicons
        .iter()
        // Search for an ico first
        .find(|favicon| favicon.ty.essence_str().eq("image/x-icon"))
        // Fallback to whatever is first
        .or_else(|| favicons.first())
}

/// Visit <title/> tags in the document
fn visit_title_tag<'doc>(
    state: &mut WebsiteDocumentState,
    parser: &Parser<'doc>,
    tag: &HTMLTag<'doc>,
) {
    let value = tag.inner_text(parser).to_string();
    state.title = Some(value);
}

/// Visit metadata tags in the document like:
///
/// <meta name="description" content="Website title" />
/// <meta property="og:title" content="Website title" />
/// <meta property="og:image" content="https://example.com/image.jpg" />
/// <meta property="og:description"Website description" />
fn visit_meta_tag<'doc>(state: &mut WebsiteDocumentState, tag: &HTMLTag<'doc>) {
    let attributes = tag.attributes();
    let property = match attributes.get("property").flatten() {
        Some(value) => value.as_bytes(),
        None => match attributes.get("name").flatten() {
            Some(value) => value.as_bytes(),
            None => return,
        },
    };

    fn get_content_value<'doc>(attributes: &tl::Attributes<'doc>) -> Option<String> {
        attributes
            .get("content")
            .flatten()
            .map(|value| value.as_utf8_str().to_string())
    }

    match property {
        b"description" => {
            if let Some(content) = get_content_value(attributes) {
                state.description = Some(content);
            }
        }
        b"og:title" => {
            if let Some(content) = get_content_value(attributes) {
                state.og_title = Some(content);
            }
        }
        b"og:description" => {
            if let Some(content) = get_content_value(attributes) {
                state.og_description = Some(content);
            }
        }
        b"og:image" => {
            if let Some(content) = get_content_value(attributes) {
                state.og_image = Some(content);
            }
        }
        _ => {}
    }
}

/// Visit a link tag attempt to find a favicon image file link:
///
/// <link rel="icon" type="image/x-icon" href="/images/favicon.ico">
/// <link rel="shortcut icon" type="image/x-icon" href="/images/favicon.ico">
fn visit_link_tag(state: &mut WebsiteDocumentState, tag: &HTMLTag<'_>) {
    let attributes = tag.attributes();

    let rel = attributes.get("rel").flatten().map(tl::Bytes::as_bytes);

    // Only match icon link
    if !matches!(rel, Some(b"icon" | b"shortcut icon")) {
        return;
    }

    let mime = attributes
        .get("type")
        .flatten()
        .and_then(|value| Mime::from_str(value.as_utf8_str().as_ref()).ok());

    // Ignore missing or invalid mimes
    let ty = match mime {
        Some(value) => value,
        None => return,
    };

    let href = attributes
        .get("href")
        .flatten()
        .map(|value| value.as_utf8_str().to_string());

    // Ignore missing href
    let href = match href {
        Some(value) => value,
        None => return,
    };

    let sizes = attributes
        .get("sizes")
        .flatten()
        .map(|value| value.as_utf8_str().to_string());

    state.favicons.push(Favicon { ty, sizes, href });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_website_metadata_all_fields() {
        let html = r#"
            <html>
                <head>
                    <title>Test Title</title>
                    <meta name="description" content="Fallback description" />
                    <meta property="og:title" content="OG Title" />
                    <meta property="og:description" content="OG Description" />
                    <meta property="og:image" content="https://example.com/image.png" />
                    <link rel="icon" type="image/x-icon" href="/favicon.ico" sizes="16x16" />
                </head>
            </html>
        "#;

        let metadata = parse_website_metadata(html).expect("Failed to parse metadata");

        assert_eq!(metadata.title, Some("Test Title".to_string()));
        assert_eq!(metadata.og_title, Some("OG Title".to_string()));
        assert_eq!(metadata.og_description, Some("OG Description".to_string()));
        assert_eq!(
            metadata.og_image,
            Some("https://example.com/image.png".to_string())
        );
        assert_eq!(metadata.favicons.len(), 1);
        let favicon = &metadata.favicons[0];
        assert_eq!(favicon.ty, mime::Mime::from_str("image/x-icon").unwrap());
        assert_eq!(favicon.href, "/favicon.ico");
        assert_eq!(favicon.sizes, Some("16x16".to_string()));
    }

    #[test]
    fn test_parse_website_metadata_fallback_description() {
        let html = r#"
            <html>
                <head>
                    <title>Test Title</title>
                    <meta name="description" content="Fallback description" />
                </head>
            </html>
        "#;

        let metadata = parse_website_metadata(html).expect("Failed to parse metadata");

        assert_eq!(
            metadata.og_description,
            Some("Fallback description".to_string())
        );
    }

    #[test]
    fn test_parse_website_metadata_missing_tags() {
        let html = r"
            <html>
                <head>
                    <!-- Empty head -->
                </head>
            </html>
        ";

        let metadata = parse_website_metadata(html).expect("Failed to parse metadata");

        assert!(metadata.title.is_none());
        assert!(metadata.og_title.is_none());
        assert!(metadata.og_description.is_none());
        assert!(metadata.og_image.is_none());
        assert!(metadata.favicons.is_empty());
    }

    #[test]
    fn test_determine_best_favicon_prefers_ico() {
        let favicons = vec![
            Favicon {
                ty: mime::Mime::from_str("image/png").unwrap(),
                href: "/favicon.png".to_string(),
                sizes: Some("32x32".to_string()),
            },
            Favicon {
                ty: mime::Mime::from_str("image/x-icon").unwrap(),
                href: "/favicon.ico".to_string(),
                sizes: Some("16x16".to_string()),
            },
        ];

        let best = determine_best_favicon(&favicons);
        assert!(best.is_some());
        assert_eq!(best.unwrap().href, "/favicon.ico");
    }

    #[test]
    fn test_determine_best_favicon_fallback() {
        let favicons = vec![Favicon {
            ty: mime::Mime::from_str("image/png").unwrap(),
            href: "/favicon.png".to_string(),
            sizes: None,
        }];

        let best = determine_best_favicon(&favicons);
        assert!(best.is_some());
        assert_eq!(best.unwrap().href, "/favicon.png");
    }

    #[test]
    fn test_determine_best_favicon_none() {
        let favicons = vec![];
        let best = determine_best_favicon(&favicons);
        assert!(best.is_none());
    }
}
