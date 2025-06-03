use crate::{files::generated::QueuedUpload, files::upload_file::ProcessingConfig};
use base64::{prelude::BASE64_STANDARD, Engine};
use bytes::Bytes;
use docbox_database::models::generated_file::GeneratedFileType;
use docbox_search::models::DocumentPage;
use mail_parser::{Address, MessageParser, MimeHeaders};
use mime::Mime;
use serde::Serialize;

use super::{AdditionalProcessingFile, ProcessingError, ProcessingIndexMetadata, ProcessingOutput};

pub const EXPERIMENTAL_EMAIL_PARSING: bool = true;

/// Checks if the provided mime is for an email
pub fn is_mail_mime(mime: &Mime) -> bool {
    mime.essence_str() == "message/rfc822"
}

/// JSON document version of the email metadata, extracts
#[derive(Debug, Serialize)]
pub struct EmailMetadataDocument {
    /// Source of the email
    from: EmailEntity,
    /// Destination of the email
    to: Vec<EmailEntity>,
    /// cc'ed emails
    cc: Vec<EmailEntity>,
    /// bcc'ed emails
    bcc: Vec<EmailEntity>,
    /// Email subject line
    subject: Option<String>,
    /// Send date of the email
    date: Option<String>,
    /// Optional message ID
    message_id: Option<String>,
    /// Collection of headers
    headers: Vec<EmailHeader>,
    /// List of attachments
    attachments: Vec<EmailAttachment>,
}

#[derive(Debug, Serialize)]
pub struct EmailAttachment {
    /// Name of the attachment
    name: String,
    length: usize,
    mime: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EmailHeader {
    name: String,
    value: String,
}

/// Optional address and name combination, usually at least one part
/// of this exists, this is used for headers like To, From, ..etc
#[derive(Debug, Clone, Serialize)]
pub struct EmailEntity {
    name: Option<String>,
    address: Option<String>,
}

/// Turns a [Address] into a collection of email entities
fn map_email_address(address: Option<&Address<'_>>) -> Vec<EmailEntity> {
    let address = match address {
        Some(value) => value,
        None => return Vec::new(),
    };

    match address {
        Address::List(addresses) => addresses
            .iter()
            .map(|value| EmailEntity {
                address: value.address().map(|value| value.to_string()),
                name: value.name().map(|value| value.to_string()),
            })
            .collect(),
        Address::Group(groups) => groups
            .iter()
            .flat_map(|group| group.addresses.iter())
            .map(|value| EmailEntity {
                address: value.address().map(|value| value.to_string()),
                name: value.name().map(|value| value.to_string()),
            })
            .collect(),
    }
}

pub fn process_email(
    config: &Option<ProcessingConfig>,
    file_bytes: &[u8],
) -> Result<ProcessingOutput, ProcessingError> {
    let is_allowed_attachments = config
        .as_ref()
        // Config is nothing or
        .is_none_or(|config| {
            // Email config is nothing or
            config
                .email
                .as_ref()
                // Skip attachments is specified and true
                .is_none_or(|email| email.skip_attachments.is_none_or(|value| !value))
        });

    let parser = MessageParser::default();
    let message = match parser.parse(file_bytes) {
        Some(value) => value,
        None => {
            // Nothing could be extracted from the file
            return Ok(ProcessingOutput::default());
        }
    };

    let from = map_email_address(message.from());

    let from = from
        .first()
        // Email must have at least one sender
        .ok_or_else(|| {
            ProcessingError::MalformedFile("email must have at least one sender".to_string())
        })?
        .clone();

    let to = map_email_address(message.to());
    let cc = map_email_address(message.cc());
    let bcc = map_email_address(message.bcc());

    let subject = message.subject().map(|value| value.to_string());
    let date = message
        .date()
        // Turn the date into an ISO date
        .map(|value| value.to_rfc3339());
    let message_id = message.message_id().map(|value| value.to_string());

    let headers: Vec<_> = message
        .headers_raw()
        .map(|(name, value)| EmailHeader {
            name: name.to_string(),
            value: value.to_string(),
        })
        .collect();

    let mut attachments: Vec<EmailAttachment> = Vec::new();
    let mut additional_files: Vec<AdditionalProcessingFile> = Vec::new();

    let text_body = message.text_bodies().next().map(|body| body.contents());

    // Get the HTML body
    let mut html_body = message
        .html_bodies()
        .next()
        .and_then(|body| body.text_contents())
        .map(|value| value.to_string());

    for attachment in message.attachments() {
        let name = match attachment.attachment_name().map(|value| value.to_string()) {
            Some(value) => value,
            None => {
                tracing::warn!("ignoring attachment without name");
                continue;
            }
        };

        let length = attachment.len();
        let raw_mime = match attachment
            .content_type()
            .map(|value| match value.subtype() {
                Some(subtype) => format!("{}/{}", value.c_type, subtype),
                None => format!("{}", value.c_type),
            }) {
            Some(value) => value,
            None => {
                tracing::warn!(?name, ?length, "ignoring attachment without mime type");
                continue;
            }
        };

        let mime: Mime = match raw_mime.parse() {
            Ok(value) => value,
            Err(cause) => {
                tracing::error!(?cause, ?raw_mime, "invalid email attachment file mime type");
                continue;
            }
        };

        let is_inline = attachment
            .content_disposition()
            .is_some_and(|value| value.is_inline());

        // For inline attachments with a content_id we inline them as base64 strings
        // directly into the email content
        if let (true, Some(content_id), Some(html_body)) =
            (is_inline, attachment.content_id(), html_body.as_mut())
        {
            // Create a data URL for the content
            let data = attachment.contents();
            let base64_data = BASE64_STANDARD.encode(data);
            let data_uri = format!("data:{};base64,{}", raw_mime, base64_data);

            let key = format!("cid:{content_id}");

            // Replace usages of the CID with the inline variant
            let new_body = html_body.replace(&key, &data_uri);
            *html_body = new_body;
            continue;
        }

        attachments.push(EmailAttachment {
            name: name.clone(),
            length,
            mime: raw_mime,
        });

        // Capture attachments if allowed
        if is_allowed_attachments {
            let bytes = attachment.contents();
            let bytes = Bytes::copy_from_slice(bytes);
            additional_files.push(AdditionalProcessingFile {
                fixed_id: None,
                name,
                mime,
                bytes,
            });
        }
    }

    let document = EmailMetadataDocument {
        from,
        to,
        cc,
        bcc,
        subject,
        date,
        message_id,
        headers,
        attachments,
    };

    let metadata_bytes = match serde_json::to_vec(&document) {
        Ok(value) => value,
        Err(cause) => {
            tracing::error!(?cause, "failed to serialize email json metadata document");
            return Err(ProcessingError::InternalServerError);
        }
    };

    let pages = text_body
        .map(|value| String::from_utf8_lossy(value).to_string())
        .map(|value| {
            vec![DocumentPage {
                content: value,
                page: 0,
            }]
        });

    let index_metadata = ProcessingIndexMetadata { pages };
    let mut upload_queue = vec![QueuedUpload::new(
        mime::APPLICATION_JSON,
        GeneratedFileType::Metadata,
        metadata_bytes.into(),
    )];

    if let Some(html_body) = html_body {
        upload_queue.push(QueuedUpload::new(
            mime::TEXT_HTML,
            GeneratedFileType::HtmlContent,
            html_body.into_bytes().into(),
        ));
    }

    if let Some(text_body) = text_body {
        upload_queue.push(QueuedUpload::new(
            mime::TEXT_PLAIN,
            GeneratedFileType::TextContent,
            text_body.to_vec().into(),
        ));
    }

    Ok(ProcessingOutput {
        encrypted: false,
        additional_files,
        index_metadata: Some(index_metadata),
        upload_queue,
    })
}
