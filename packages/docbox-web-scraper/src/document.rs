//! # Document
//!
//! HTML document related logic for extracting information scraped from remote
//! HTML pages such as OGP metadata <title/> tags etc

use std::str::FromStr;

use anyhow::Context;
use mime::Mime;
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
    pub ty: Mime,
    pub _sizes: Option<String>,
    pub href: String,
}

#[derive(Default)]
struct WebsiteDocumentState {
    title: Option<String>,
    description: Option<String>,
    og_title: Option<String>,
    og_description: Option<String>,
    og_image: Option<String>,
    favicons: Vec<Favicon>,
}

/// Connects to a website reading the HTML contents, extracts the metadata
/// required from the <head/> element
pub async fn get_website_metadata(
    client: &reqwest::Client,
    url: &Url,
) -> anyhow::Result<WebsiteMetadata> {
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
        .context("failed to request resource")?
        .error_for_status()
        .context("resource responded with error")?;

    // Read response text
    let text = response
        .text()
        .await
        .context("failed to read resource response")?;

    parse_website_metadata(&text)
}

pub fn parse_website_metadata(html: &str) -> anyhow::Result<WebsiteMetadata> {
    let dom = tl::parse(html, tl::ParserOptions::default())
        .context("failed to parse resource response")?;

    let parser = dom.parser();

    // Find the head element
    let head = dom
        .query_selector("head")
        .context("failed to query page head")?
        .next()
        .context("page missing head")?
        .get(parser)
        .context("failed to parse head")?;

    let mut state = WebsiteDocumentState::default();

    let children = head.children().context("head missing children")?;
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
fn visit_link_tag<'doc>(state: &mut WebsiteDocumentState, tag: &HTMLTag<'doc>) {
    let attributes = tag.attributes();

    let rel = attributes
        .get("rel")
        .flatten()
        .map(|value| value.as_bytes());

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

    state.favicons.push(Favicon {
        ty,
        href,
        _sizes: sizes,
    })
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
        assert_eq!(favicon._sizes, Some("16x16".to_string()));
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
        let html = r#"
            <html>
                <head>
                    <!-- Empty head -->
                </head>
            </html>
        "#;

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
                _sizes: Some("32x32".to_string()),
            },
            Favicon {
                ty: mime::Mime::from_str("image/x-icon").unwrap(),
                href: "/favicon.ico".to_string(),
                _sizes: Some("16x16".to_string()),
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
            _sizes: None,
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
