use tl::{Node, Parser};

/// Tags that are considered blocks and should have a newline appended
const BLOCK_TAGS: &[&str] = &[
    "div",
    "p",
    "section",
    "article",
    "header",
    "footer",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "ul",
    "ol",
    "li",
    "pre",
    "blockquote",
    "table",
    "tr",
    "td",
    "th",
    "br",
];

/// Convert the provided `html` into a text representation maintaining
/// the newlines that would be produced by block elements
pub fn html_to_text(html: &str) -> Result<String, tl::ParseError> {
    let dom = tl::parse(html, tl::ParserOptions::default())?;

    let parser = dom.parser();

    let mut output = String::new();

    for child in dom.children() {
        let node = match child.get(parser) {
            Some(value) => value,
            None => continue,
        };
        extract_text(parser, node, &mut output);
    }

    let decoded = html_escape::decode_html_entities(&output);
    Ok(decoded.to_string())
}

fn extract_text<'doc>(parser: &Parser<'doc>, node: &Node<'doc>, out: &mut String) {
    match node {
        Node::Raw(text) => {
            out.push_str(text.as_utf8_str().as_ref());
        }
        Node::Tag(tag) => {
            let tag_name = tag.name().as_utf8_str();
            let is_block = BLOCK_TAGS.contains(&tag_name.as_ref());

            let children = tag.children();
            let children = children.top();

            for child in children.as_slice() {
                let child = match child.get(parser) {
                    Some(value) => value,
                    None => continue,
                };
                extract_text(parser, child, out);
            }

            if is_block {
                out.push('\n');
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_html_entities_named() {
        let html = "<p>Tom &amp; Jerry &lt;3 &quot;quotes&quot; &apos;single&apos;</p>";
        let text = html_to_text(html).unwrap();
        assert_eq!(text, "Tom & Jerry <3 \"quotes\" 'single'\n");
    }

    #[test]
    fn test_html_entities_numeric_decimal() {
        let html = "<p>Smile &#128512; &#169; &#174;</p>";
        let text = html_to_text(html).unwrap();
        assert_eq!(text, "Smile 😀 © ®\n");
    }

    #[test]
    fn test_html_entities_numeric_hex() {
        let html = "<p>Heart &#x2764; &#x1F600;</p>";
        let text = html_to_text(html).unwrap();
        assert_eq!(text, "Heart ❤ 😀\n");
    }

    #[test]
    fn test_mixed_html_entities() {
        let html = "<p>Mix &amp; match &#38; &#x26; &lt; &#60;</p>";
        let text = html_to_text(html).unwrap();
        assert_eq!(text, "Mix & match & & < <\n");
    }

    #[test]
    fn test_html_entities_in_nested_tags() {
        let html = "<div>Price: &dollar;100 <span>Tax: &#37;10</span></div>";
        let text = html_to_text(html).unwrap();
        assert_eq!(text, "Price: $100 Tax: %10\n");
    }

    #[test]
    fn test_simple_paragraph() {
        let html = "<p>Hello, <strong>world</strong>!</p>";
        let text = html_to_text(html).unwrap();
        assert_eq!(text, "Hello, world!\n");
    }

    #[test]
    fn test_simple_paragraph_with_br() {
        let html = "<p>Hello, <strong>world</strong>!</p><br>";
        let text = html_to_text(html).unwrap();
        assert_eq!(text, "Hello, world!\n\n");
    }

    #[test]
    fn test_multiple_block_elements() {
        let html = "<h1>Title</h1><p>Paragraph 1.</p><p>Paragraph 2.</p>";
        let text = html_to_text(html).unwrap();
        assert_eq!(text, "Title\nParagraph 1.\nParagraph 2.\n");
    }

    #[test]
    fn test_nested_blocks() {
        let html = "<div><h1>Header</h1><p>Paragraph <em>with</em> emphasis.</p></div>";
        let text = html_to_text(html).unwrap();
        assert_eq!(text, "Header\nParagraph with emphasis.\n\n");
    }

    #[test]
    fn test_list_items() {
        let html = "<ul><li>Item 1</li><li>Item 2</li></ul>";
        let text = html_to_text(html).unwrap();
        assert_eq!(text, "Item 1\nItem 2\n\n");
    }

    #[test]
    fn test_mixed_inline_and_block() {
        let html = "<div>Block <span>inline</span> text.</div>";
        let text = html_to_text(html).unwrap();
        assert_eq!(text, "Block inline text.\n");
    }

    #[test]
    fn test_table_with_tr_td() {
        let html = "<table><tr><td>Cell 1</td><td>Cell 2</td></tr><tr><td>Cell 3</td><td>Cell 4</td></tr></table>";
        let text = html_to_text(html).unwrap();
        assert_eq!(text, "Cell 1\nCell 2\n\nCell 3\nCell 4\n\n\n");
    }

    #[test]
    fn test_header_footer() {
        let html = "<header>Header content</header><footer>Footer content</footer>";
        let text = html_to_text(html).unwrap();
        assert_eq!(text, "Header content\nFooter content\n");
    }

    #[test]
    fn test_blockquote() {
        let html = "<blockquote>This is a quote.</blockquote>";
        let text = html_to_text(html).unwrap();
        assert_eq!(text, "This is a quote.\n");
    }

    #[test]
    fn test_preformatted_text() {
        let html = "<pre>Code\nblock</pre>";
        let text = html_to_text(html).unwrap();
        assert_eq!(text, "Code\nblock\n");
    }

    #[test]
    fn test_heading_levels() {
        let html = "<h1>H1</h1><h2>H2</h2><h3>H3</h3><h4>H4</h4><h5>H5</h5><h6>H6</h6>";
        let text = html_to_text(html).unwrap();
        assert_eq!(text, "H1\nH2\nH3\nH4\nH5\nH6\n");
    }

    #[test]
    fn test_section_article() {
        let html = "<section>Section content</section><article>Article content</article>";
        let text = html_to_text(html).unwrap();
        assert_eq!(text, "Section content\nArticle content\n");
    }

    #[test]
    fn test_empty_tags() {
        let html = "<p></p><div></div><section></section>";
        let text = html_to_text(html).unwrap();
        assert_eq!(text, "\n\n\n");
    }

    #[test]
    fn test_nested_blocks_with_inline() {
        let html = "<div><p>Paragraph with <strong>bold</strong> text.</p><footer>Footer <em>content</em>.</footer></div>";
        let text = html_to_text(html).unwrap();
        assert_eq!(text, "Paragraph with bold text.\nFooter content.\n\n");
    }

    #[test]
    fn test_br_inside_paragraph() {
        let html = "<p>Line 1<br>Line 2<br>Line 3</p>";
        let text = html_to_text(html).unwrap();
        assert_eq!(text, "Line 1\nLine 2\nLine 3\n");
    }
}
