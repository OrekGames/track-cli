// ============================================================================
// Content Format Conversion
// ============================================================================

/// Convert Confluence storage format (XHTML) to plain text.
///
/// Tags are stripped (line breaks and closing block tags become newlines) and
/// entities in text are decoded. CDATA sections, which hold code macro bodies,
/// are copied verbatim: their markup and entities are literal code.
pub(crate) fn storage_to_text(storage: &str) -> String {
    const CDATA_START: &str = "<![CDATA[";
    const CDATA_END: &str = "]]>";

    let mut output = String::with_capacity(storage.len());
    let mut rest = storage;
    while let Some(tag_start) = rest.find('<') {
        output.push_str(&decode_html_entities(&rest[..tag_start]));
        rest = &rest[tag_start..];
        if let Some(cdata) = rest.strip_prefix(CDATA_START) {
            let (body, after) = cdata.split_once(CDATA_END).unwrap_or((cdata, ""));
            output.push_str(body);
            rest = after;
        } else {
            let tag_end = rest.find('>').map_or(rest.len(), |i| i + 1);
            output.push_str(text_for_tag(&rest[..tag_end]));
            rest = &rest[tag_end..];
        }
    }
    output.push_str(&decode_html_entities(rest));

    // Clean up multiple newlines
    while output.contains("\n\n\n") {
        output = output.replace("\n\n\n", "\n\n");
    }

    output.trim().to_string()
}

/// The plain-text stand-in for a stripped tag: line breaks and the ends of
/// block elements become newlines (two after major headings), anything else
/// disappears.
fn text_for_tag(tag: &str) -> &'static str {
    let name = tag
        .trim_start_matches('<')
        .trim_end_matches('>')
        .trim_end_matches('/')
        .split_whitespace()
        .next()
        .unwrap_or_default();
    match name.to_ascii_lowercase().as_str() {
        "br" | "/p" | "/div" | "/li" | "/h4" | "/h5" | "/h6" => "\n",
        "/h1" | "/h2" | "/h3" => "\n\n",
        _ => "",
    }
}

/// Decode the HTML entities Confluence emits for plain text. `&amp;` goes
/// last so an escaped entity such as `&amp;lt;` decodes once, to `&lt;`.
fn decode_html_entities(text: &str) -> String {
    text.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
}

/// Convert Markdown to Confluence storage format.
pub(crate) fn markdown_to_storage(markdown: &str) -> String {
    use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

    if looks_like_confluence_storage(markdown) {
        return markdown.to_string();
    }

    let parser = Parser::new_ext(
        markdown,
        Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS,
    );

    let mut result = String::new();
    let mut code_block: Option<(String, String)> = None;
    let mut image: Option<(String, String)> = None;
    let mut in_table_head = false;

    for event in parser {
        if let Some((_, body)) = code_block.as_mut() {
            match event {
                Event::End(TagEnd::CodeBlock) => {
                    let (language, body) = code_block.take().expect("code block is open");
                    result.push_str(&storage_code_macro(&language, &body));
                }
                Event::Text(text)
                | Event::Code(text)
                | Event::Html(text)
                | Event::InlineHtml(text)
                | Event::InlineMath(text)
                | Event::DisplayMath(text) => body.push_str(&text),
                Event::SoftBreak | Event::HardBreak => body.push('\n'),
                _ => {}
            }
            continue;
        }

        if let Some((_, alt_text)) = image.as_mut() {
            match event {
                Event::End(TagEnd::Image) => {
                    let (url, alt_text) = image.take().expect("image is open");
                    result.push_str(&format!(
                        "<ac:image ac:alt=\"{}\"><ri:url ri:value=\"{}\" /></ac:image>",
                        html_escape(&alt_text),
                        html_escape(&url)
                    ));
                }
                Event::Text(text) | Event::Code(text) => alt_text.push_str(&text),
                Event::SoftBreak | Event::HardBreak => alt_text.push(' '),
                _ => {}
            }
            continue;
        }

        match event {
            Event::Start(tag) => match tag {
                Tag::Paragraph => result.push_str("<p>"),
                Tag::Heading { level, .. } => {
                    result.push_str(&format!("<{}>", heading_tag(level)));
                }
                Tag::BlockQuote(_) => result.push_str("<blockquote>"),
                Tag::CodeBlock(kind) => {
                    let language = match kind {
                        CodeBlockKind::Fenced(lang) => lang.to_string(),
                        CodeBlockKind::Indented => String::new(),
                    };
                    code_block = Some((language, String::new()));
                }
                Tag::List(Some(start)) => {
                    if start == 1 {
                        result.push_str("<ol>");
                    } else {
                        result.push_str(&format!("<ol start=\"{start}\">"));
                    }
                }
                Tag::List(None) => result.push_str("<ul>"),
                Tag::Item => result.push_str("<li>"),
                Tag::Emphasis => result.push_str("<em>"),
                Tag::Strong => result.push_str("<strong>"),
                Tag::Strikethrough => result.push_str("<del>"),
                Tag::Superscript => result.push_str("<sup>"),
                Tag::Subscript => result.push_str("<sub>"),
                Tag::Link { dest_url, .. } => {
                    result.push_str(&format!("<a href=\"{}\">", html_escape(&dest_url)));
                }
                Tag::Image { dest_url, .. } => {
                    image = Some((dest_url.to_string(), String::new()));
                }
                Tag::Table(_) => result.push_str("<table><tbody>"),
                Tag::TableHead => in_table_head = true,
                Tag::TableRow => result.push_str("<tr>"),
                Tag::TableCell => {
                    result.push_str(if in_table_head { "<th>" } else { "<td>" });
                }
                Tag::DefinitionList => result.push_str("<dl>"),
                Tag::DefinitionListTitle => result.push_str("<dt>"),
                Tag::DefinitionListDefinition => result.push_str("<dd>"),
                Tag::HtmlBlock | Tag::FootnoteDefinition(_) | Tag::MetadataBlock(_) => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Paragraph => result.push_str("</p>"),
                TagEnd::Heading(level) => {
                    result.push_str(&format!("</{}>", heading_tag(level)));
                }
                TagEnd::BlockQuote(_) => result.push_str("</blockquote>"),
                TagEnd::CodeBlock => {}
                TagEnd::List(true) => result.push_str("</ol>"),
                TagEnd::List(false) => result.push_str("</ul>"),
                TagEnd::Item => result.push_str("</li>"),
                TagEnd::Emphasis => result.push_str("</em>"),
                TagEnd::Strong => result.push_str("</strong>"),
                TagEnd::Strikethrough => result.push_str("</del>"),
                TagEnd::Superscript => result.push_str("</sup>"),
                TagEnd::Subscript => result.push_str("</sub>"),
                TagEnd::Link => result.push_str("</a>"),
                TagEnd::Image => {}
                TagEnd::Table => result.push_str("</tbody></table>"),
                TagEnd::TableHead => in_table_head = false,
                TagEnd::TableRow => result.push_str("</tr>"),
                TagEnd::TableCell => {
                    result.push_str(if in_table_head { "</th>" } else { "</td>" });
                }
                TagEnd::DefinitionList => result.push_str("</dl>"),
                TagEnd::DefinitionListTitle => result.push_str("</dt>"),
                TagEnd::DefinitionListDefinition => result.push_str("</dd>"),
                TagEnd::HtmlBlock | TagEnd::FootnoteDefinition | TagEnd::MetadataBlock(_) => {}
            },
            Event::Text(text) => result.push_str(&html_escape(&text)),
            Event::Code(text) => {
                result.push_str(&format!("<code>{}</code>", html_escape(&text)));
            }
            Event::InlineMath(text) => {
                result.push_str(&format!("<code>{}</code>", html_escape(&text)));
            }
            Event::DisplayMath(text) => {
                result.push_str("<p><code>");
                result.push_str(&html_escape(&text));
                result.push_str("</code></p>");
            }
            Event::Html(html) | Event::InlineHtml(html) => result.push_str(&html),
            Event::FootnoteReference(reference) => {
                result.push_str(&format!("<sup>{}</sup>", html_escape(&reference)));
            }
            Event::SoftBreak => result.push('\n'),
            Event::HardBreak => result.push_str("<br/>"),
            Event::Rule => result.push_str("<hr/>"),
            Event::TaskListMarker(checked) => {
                result.push_str(if checked { "[x] " } else { "[ ] " });
            }
        }
    }

    result
}

fn looks_like_confluence_storage(input: &str) -> bool {
    let trimmed = input.trim_start();
    let lower = trimmed.to_ascii_lowercase();

    [
        "<ac:",
        "<ri:",
        "<table",
        "<tbody",
        "<thead",
        "<tr",
        "<td",
        "<th",
        "<p",
        "<h1",
        "<h2",
        "<h3",
        "<h4",
        "<h5",
        "<h6",
        "<ul",
        "<ol",
        "<blockquote",
        "<pre",
    ]
    .iter()
    .any(|prefix| lower.starts_with(prefix))
}

fn heading_tag(level: pulldown_cmark::HeadingLevel) -> &'static str {
    match level {
        pulldown_cmark::HeadingLevel::H1 => "h1",
        pulldown_cmark::HeadingLevel::H2 => "h2",
        pulldown_cmark::HeadingLevel::H3 => "h3",
        pulldown_cmark::HeadingLevel::H4 => "h4",
        pulldown_cmark::HeadingLevel::H5 => "h5",
        pulldown_cmark::HeadingLevel::H6 => "h6",
    }
}

fn storage_code_macro(language: &str, body: &str) -> String {
    let mut result = String::from("<ac:structured-macro ac:name=\"code\">");
    let language = language.trim();

    if !language.is_empty() {
        result.push_str(&format!(
            "<ac:parameter ac:name=\"language\">{}</ac:parameter>",
            html_escape(language)
        ));
    }

    result.push_str("<ac:plain-text-body><![CDATA[");
    result.push_str(&escape_cdata(body));
    result.push_str("]]></ac:plain-text-body></ac:structured-macro>");
    result
}

fn escape_cdata(input: &str) -> String {
    input.replace("]]>", "]]]]><![CDATA[>")
}

pub(crate) fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_to_storage_renders_rich_markdown() {
        let markdown = r#"# Title

| A | B |
|---|---|
| 1<br>2 | **bold** |

```bash
echo hi
```

> **Note:** something
"#;

        let storage = markdown_to_storage(markdown);

        assert!(storage.contains("<h1>Title</h1>"));
        assert!(storage.contains("<table><tbody>"));
        assert!(storage.contains("<th>A</th>"));
        assert!(storage.contains("<td>1<br>2</td>"));
        assert!(storage.contains("<td><strong>bold</strong></td>"));
        assert!(storage.contains("<ac:structured-macro ac:name=\"code\">"));
        assert!(storage.contains("<ac:parameter ac:name=\"language\">bash</ac:parameter>"));
        assert!(storage.contains("<ac:plain-text-body><![CDATA[echo hi\n]]>"));
        assert!(
            storage.contains("<blockquote><p><strong>Note:</strong> something</p></blockquote>")
        );
        assert!(
            !storage.contains("| A | B |"),
            "table Markdown should be converted, got: {storage}"
        );
        assert!(
            !storage.contains("```"),
            "fenced code markers should not be emitted, got: {storage}"
        );
    }

    #[test]
    fn markdown_to_storage_preserves_native_storage_xml() {
        let native_storage = r#"<table><tbody><tr><th>A</th><th>B</th></tr><tr><td>1</td><td>2</td></tr></tbody></table>
<ac:structured-macro ac:name="info"><ac:rich-text-body><p>Hi</p></ac:rich-text-body></ac:structured-macro>"#;

        let storage = markdown_to_storage(native_storage);

        assert_eq!(storage, native_storage);
        assert!(!storage.contains("&lt;table&gt;"));
        assert!(!storage.contains("&lt;ac:structured-macro"));
    }

    #[test]
    fn markdown_to_storage_converts_markdown_that_mentions_storage_tags() {
        let storage = markdown_to_storage("# Title\n\n`<ac:structured-macro>`");

        assert!(storage.contains("<h1>Title</h1>"));
        assert!(storage.contains("<code>&lt;ac:structured-macro&gt;</code>"));
    }

    #[test]
    fn markdown_to_storage_nested_bullet_list_keeps_parent_text_out_of_child_code() {
        let markdown =
            "- Re-verify consumers:\n  - `grep -rn \"foo\" src/`\n  - `grep -rn \"bar\" src/`";

        let storage = markdown_to_storage(markdown);

        assert!(
            storage.contains("<ul><li>Re-verify consumers:<ul>"),
            "parent text should precede nested list, got: {storage}"
        );
        assert!(storage.contains("<code>grep -rn &quot;foo&quot; src/</code>"));
        assert!(storage.contains("<code>grep -rn &quot;bar&quot; src/</code>"));
        assert!(
            !storage.contains("<code>Re-verify consumers:"),
            "child code nodes should not include parent text, got: {storage}"
        );
    }

    #[test]
    fn markdown_to_storage_nested_ordered_list_keeps_parent_text_before_children() {
        let markdown = "1. Prepare release:\n   1. Build artifacts\n   2. Upload artifacts";

        let storage = markdown_to_storage(markdown);

        assert!(
            storage.contains("<ol><li>Prepare release:<ol>"),
            "parent text should precede nested ordered list, got: {storage}"
        );
        assert!(storage.contains("<li>Build artifacts</li>"));
        assert!(storage.contains("<li>Upload artifacts</li>"));
        assert!(
            !storage.contains("Prepare release:<ol><li>Prepare release:"),
            "child item text should remain independent, got: {storage}"
        );
    }

    #[test]
    fn test_storage_to_text_correct() {
        // Test: CDATA blocks are completely preserved, even if they contain comparative operators
        let storage_xml = "<ac:structured-macro ac:name=\"code\"><ac:plain-text-body><![CDATA[if x < 5 && y > 3 { println!(\"hi\"); }]]></ac:plain-text-body></ac:structured-macro>";
        let plain_text = storage_to_text(storage_xml);
        assert_eq!(plain_text, "if x < 5 && y > 3 { println!(\"hi\"); }");
    }

    #[test]
    fn storage_to_text_decodes_escaped_entities_only_once() {
        // The page text is literally "&lt;br&gt;", stored with its `&` escaped.
        assert_eq!(
            storage_to_text("<p>Write &amp;lt;br&amp;gt; for a break</p>"),
            "Write &lt;br&gt; for a break"
        );
    }

    #[test]
    fn storage_to_text_keeps_cdata_markup_and_entities_verbatim() {
        let storage_xml = "<p>Example:</p><ac:structured-macro ac:name=\"code\"><ac:plain-text-body><![CDATA[<p>a &amp; b</p><br/>]]></ac:plain-text-body></ac:structured-macro>";
        assert_eq!(
            storage_to_text(storage_xml),
            "Example:\n<p>a &amp; b</p><br/>"
        );
    }
}
