//! Service that handles scraping websites.
//!
//! The service obtains the following:
//! - Favicon
//! - Page title
//! - OGP/Social Metadata (https://ogp.me/)
//!
//! It downloads the desired thumbnail and an OGP image if available
//! and stores it as [Bytes] in memory. Keeps an in-memory cache for
//! 48h of already visited websites

use anyhow::{anyhow, Context};
use bytes::Bytes;
use http::{HeaderMap, HeaderValue};
use mime::Mime;
use moka::{future::Cache, policy::EvictionPolicy};
use reqwest::{header::CONTENT_TYPE, Proxy, Url};
use serde::Serialize;
use std::{str::FromStr, time::Duration};
use tracing::{debug, error};
use url::Host;

pub type OgpHttpClient = reqwest::Client;

/// Service for looking up website metadata and storing a cached value
#[derive(Clone)]
pub struct WebsiteMetaService {
    client: OgpHttpClient,
    cache: Cache<String, ResolvedWebsiteMetadata>,
}

/// Duration to maintain site metadata for (48h)
const METADATA_CACHE_DURATION: Duration = Duration::from_secs(60 * 60 * 48);

/// Time to wait when attempting to fetch resource before timing out
const METADATA_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// Time to wait while downloading a resource before timing out (between each read of data)
const METADATA_READ_TIMEOUT: Duration = Duration::from_secs(30);

/// Environment variable to use for the server address
const HTTP_PROXY_ENV: &str = "HTTP_PROXY";

/// Checks if the provided URL uses an IP address instead of a
/// domain. We disallow these explicitly when fetching metadata
pub fn is_url_ip(url: &Url) -> bool {
    url.host()
        .is_some_and(|value| matches!(value, Host::Ipv4(_) | Host::Ipv6(_)))
}

impl WebsiteMetaService {
    /// Creates a new instance of the service, this initializes the HTTP
    /// client and creates the cache
    pub fn new() -> anyhow::Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert("user-agent", HeaderValue::from_static("DocboxLinkBot"));

        let proxy = Proxy::all(
            std::env::var(HTTP_PROXY_ENV).context("missing HTTP_PROXY environment variable")?,
        )
        .context("failed to create proxy")?;

        let client = reqwest::Client::builder()
            .proxy(proxy)
            .default_headers(headers)
            .connect_timeout(METADATA_CONNECT_TIMEOUT)
            .read_timeout(METADATA_READ_TIMEOUT)
            .build()
            .context("failed to build http client")?;

        let cache = Cache::builder()
            .time_to_idle(METADATA_CACHE_DURATION)
            .max_capacity(100)
            .eviction_policy(EvictionPolicy::tiny_lfu())
            .build();

        Ok(Self { client, cache })
    }

    /// Resolves the metadata for the website at the provided URL
    pub async fn resolve_website(&self, url: &str) -> anyhow::Result<ResolvedWebsiteMetadata> {
        // Cache hit
        if let Some(cached) = self.cache.get(url).await {
            return Ok(cached);
        }

        // Get the website metadata
        let res = get_website_metadata(&self.client, url).await?;
        let best_favicon = determine_best_favicon(&res.favicons);

        // Download the favicon
        let favicon = match best_favicon {
            Some(best_favicon) => {
                let result = download_remote_img(&self.client, url, &best_favicon.href)
                    .await
                    .context("failed to load favicon")
                    .map(|option| {
                        option.map(|(favicon_bytes, favicon_mime)| ResolvedImage {
                            bytes: favicon_bytes,
                            content_type: favicon_mime,
                        })
                    });

                match result {
                    Ok(value) => value,
                    Err(cause) => {
                        error!(%url, ?cause, "failed to resolve favicon");
                        None
                    }
                }
            }
            None => None,
        };

        // Download the OGP image
        let image = match res.og_image.as_ref() {
            Some(og_image) => {
                let result = download_remote_img(&self.client, url, og_image)
                    .await
                    .context("failed to load ogp image")
                    .map(|option| {
                        option.map(|(image_bytes, image_mime)| ResolvedImage {
                            bytes: image_bytes,
                            content_type: image_mime,
                        })
                    });

                match result {
                    Ok(value) => value,
                    Err(cause) => {
                        error!(%url, ?cause, "failed to resolve og image");
                        None
                    }
                }
            }
            None => None,
        };

        let resolved = ResolvedWebsiteMetadata {
            title: res.title,
            og_title: res.og_title,
            og_description: res.og_description,
            og_image: image,
            favicon,
        };

        // Cache the response
        self.cache.insert(url.to_string(), resolved.clone()).await;

        Ok(resolved)
    }
}

#[derive(Clone, Serialize)]
pub struct ResolvedWebsiteMetadata {
    pub title: Option<String>,
    pub og_title: Option<String>,
    pub og_description: Option<String>,

    #[serde(skip)]
    pub og_image: Option<ResolvedImage>,
    #[serde(skip)]
    pub favicon: Option<ResolvedImage>,
}

/// Represents an image that has been resolved where the
/// contents are now know and the content type as well
#[derive(Debug, Clone)]
pub struct ResolvedImage {
    pub content_type: Mime,
    pub bytes: Bytes,
}

/// Metadata extracted from a website
#[derive(Debug)]
struct WebsiteMetadata {
    title: Option<String>,
    og_title: Option<String>,
    og_description: Option<String>,
    og_image: Option<String>,
    favicons: Vec<Favicon>,
}

/// Favicon extracted from a website
#[derive(Debug, Clone)]
struct Favicon {
    ty: Mime,
    _sizes: Option<String>,
    href: String,
}

/// Determines which favicon to use from the provided list
///
/// Prefers .ico format currently then defaulting to first
/// available. At a later date might want to check the sizes
/// field
fn determine_best_favicon(favicons: &[Favicon]) -> Option<&Favicon> {
    // Search for an ico first
    if let Some(ico) = favicons
        .iter()
        .find(|favicon| favicon.ty.essence_str().eq("image/x-icon"))
    {
        return Some(ico);
    }

    // Fallback to whatever is first
    favicons.first()
}

/// Connects to a website reading the HTML contents, extracts the metadata
/// required from the <head/> element
async fn get_website_metadata(
    client: &OgpHttpClient,
    url: &str,
) -> anyhow::Result<WebsiteMetadata> {
    let mut url = reqwest::Url::parse(url).context("invalid resource url")?;

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

    let dom = tl::parse(&text, tl::ParserOptions::default())
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

    let mut title: Option<String> = None;
    let mut description: Option<String> = None;
    let mut og_title: Option<String> = None;
    let mut og_description: Option<String> = None;
    let mut og_image: Option<String> = None;
    let mut favicons: Vec<Favicon> = Vec::new();

    let children = head.children().context("head missing children")?;
    for child in children.all(parser) {
        let tag = match child.as_tag() {
            Some(tag) => tag,
            None => continue,
        };

        match tag.name().as_bytes() {
            // Extract page title tag
            b"title" => {
                let value = tag.inner_text(parser);
                title = Some(value.to_string());
            }

            // Extract metadata
            b"meta" => {
                let attributes = tag.attributes();
                let property = match attributes.get("property").flatten() {
                    Some(value) => value.as_bytes(),
                    None => match attributes.get("name").flatten() {
                        Some(value) => value.as_bytes(),
                        None => continue,
                    },
                };

                const DESCRIPTION: &[u8] = b"description";
                const OG_TITLE: &[u8] = b"og:title";
                const OG_DESCRIPTION: &[u8] = b"og:description";
                const OG_IMAGE: &[u8] = b"og:image";

                // Only work with the tags we use
                if !matches!(property, DESCRIPTION | OG_TITLE | OG_DESCRIPTION | OG_IMAGE) {
                    continue;
                }

                let content = match attributes.get("content").flatten() {
                    Some(value) => value.as_utf8_str(),
                    None => continue,
                };

                match property {
                    DESCRIPTION => {
                        description = Some(content.to_string());
                    }
                    OG_TITLE => {
                        og_title = Some(content.to_string());
                    }
                    OG_DESCRIPTION => {
                        og_description = Some(content.to_string());
                    }
                    OG_IMAGE => og_image = Some(content.to_string()),
                    _ => {}
                }
            }

            // Extract favicons
            b"link" => {
                let attributes = tag.attributes();

                let rel = attributes
                    .get("rel")
                    .flatten()
                    .map(|value| value.as_bytes());

                // Only match icon link
                if !matches!(rel, Some(b"icon" | b"shortcut icon")) {
                    continue;
                }

                let mime = attributes
                    .get("type")
                    .flatten()
                    .and_then(|value| Mime::from_str(value.as_utf8_str().as_ref()).ok());

                // Ignore missing or invalid mimes
                let ty = match mime {
                    Some(value) => value,
                    None => continue,
                };

                let href = attributes
                    .get("href")
                    .flatten()
                    .map(|value| value.as_utf8_str().to_string());

                // Ignore missing href
                let href = match href {
                    Some(value) => value,
                    None => continue,
                };

                let sizes = attributes
                    .get("sizes")
                    .flatten()
                    .map(|value| value.as_utf8_str().to_string());

                favicons.push(Favicon {
                    ty,
                    href,
                    _sizes: sizes,
                })
            }

            // Ignore other tags
            _ => {}
        }
    }

    // Fallback to description
    let og_description = og_description.or(description);

    Ok(WebsiteMetadata {
        title,
        og_title,
        og_description,
        og_image,
        favicons,
    })
}

/// Parses a base64 encoded image data URL
fn parse_data_url(data_url: &str) -> anyhow::Result<(Bytes, Mime)> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};

    // Strip the data prefix
    let data_url = data_url.strip_prefix("data:").unwrap_or(data_url);

    let (raw_mime, data_url) = data_url.split_once(';').context("invalid data url")?;

    let mime: Mime = raw_mime.parse().context("invalid mime type")?;

    // Split the data URL into metadata and base64-encoded data
    let parts: Vec<&str> = data_url.split(',').collect();
    if parts.len() != 2 {
        return Err(anyhow!("invalid data url format"));
    }

    let metadata = parts[0];
    let base64_data = parts[1];

    // Only base64 is supported
    if !metadata.contains("base64") {
        return Err(anyhow!("unhandled data url format"));
    }

    // Decode the base64 data if needed
    let data = STANDARD
        .decode(base64_data)
        .context("failed to decode data url base64")?;

    let data = Bytes::from(data);

    Ok((data, mime))
}

/// Downloads an image from a remote URL, handles resolving the URL path for
/// relative and absolute URLs
///
/// Has additional access control checks:
/// - Prevents direct IP access
/// - Aborts if the content type is not an image
///
/// Returns an option with [Some] when the image is valid and [None] when
/// the image could be loaded but was invalid (data urls)
pub async fn download_remote_img(
    client: &OgpHttpClient,
    base_url: &str,
    href: &str,
) -> anyhow::Result<Option<(Bytes, Mime)>> {
    // Handle data urls
    if href.starts_with("data:") {
        return Ok(parse_data_url(href).ok());
    }

    // Replace & encoding for query params
    let href = href.replace("&amp;", "&");
    let base_url = base_url.replace("&amp;", "&");

    let base_url = reqwest::Url::parse(&base_url).context("invalid resource url")?;

    debug!(%href, %base_url, "requesting remote image");

    // Resolve the full URL
    let favicon_url = if href.starts_with("http") {
        // If href is an absolute URL, use it directly
        Url::parse(&href)
    } else {
        // If href is a relative URL, resolve it against the base URL
        base_url.join(&href)
    }
    .context("failed to parse icon url")?;

    if is_url_ip(&favicon_url) {
        return Err(anyhow!("illegal url access"));
    }

    // Request page at URL
    let response = client
        .get(favicon_url)
        .send()
        .await
        .context("failed to request resource")?
        .error_for_status()
        .context("resource responded with error")?;

    let headers = response.headers();
    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<Mime>().ok())
        .context("remote image invalid or missing content type")?;

    if content_type.type_() != mime::IMAGE {
        return Err(anyhow!("remote image invalid content type"));
    }

    // Read response text
    let bytes = response
        .bytes()
        .await
        .context("failed to read resource response")?;

    Ok(Some((bytes, content_type)))
}

#[cfg(test)]
mod test {
    use http::{HeaderMap, HeaderValue};
    use reqwest::Client;

    use super::{determine_best_favicon, download_remote_img, get_website_metadata};

    #[tokio::test]
    #[ignore]
    async fn test_google_ogp() {
        let mut headers = HeaderMap::new();
        headers.insert("user-agent", HeaderValue::from_static("DocboxLinkBot"));

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .unwrap();

        let base_url = "https://www.youtube.com/watch?v=suhEIUapSJQ";
        let res = get_website_metadata(&client, base_url).await.unwrap();
        let best_favicon = determine_best_favicon(&res.favicons).unwrap();

        // let _bytes = download_remote_img(&client, base_url, &best_favicon.href)
        //     .await
        //     .unwrap();
        // let _bytes = download_remote_img(&client, base_url, &res.og_image.clone().unwrap())
        //     .await
        //     .unwrap();

        dbg!(&res, &best_favicon);
    }
    #[tokio::test]
    #[ignore]
    async fn test_base64_data_url() {
        let client = Client::default();

        let _bytes = download_remote_img(&client, "", "data:image/jpeg;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAB0lEQVR42mP8/wcAAwAB/8I+gQAAAABJRU5ErkJggg==").await.unwrap();

        dbg!(&_bytes);
    }
}
