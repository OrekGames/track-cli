/// Remove HTML tags from a string, returning the inner text.
fn strip_html_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

/// Convert a markdown string to an ADF document.
///
/// Parses CommonMark + GFM and maps constructs to ADF node types:
/// headings, paragraphs, bullet/ordered/task lists, blockquotes, code blocks,
/// tables, horizontal rules, images (as links), and inline marks
/// (strong, em, strikethrough, code, links).
pub(crate) fn markdown_to_adf(text: &str) -> serde_json::Value {
    use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
    use serde_json::{Value, json};

    let parser = Parser::new_ext(
        text,
        Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS | Options::ENABLE_TABLES,
    );

    // Stack of in-progress block nodes. Each entry is (node_type, content_vec).
    // We push when entering a block and pop+finalize when leaving.
    let mut block_stack: Vec<(String, Vec<Value>)> = Vec::new();
    // Completed top-level content nodes
    let mut doc_content: Vec<Value> = Vec::new();
    // Inline text/mark accumulator for the current paragraph/heading/etc.
    let mut inline_buf: Vec<Value> = Vec::new();
    // Active inline marks (stack of mark type strings)
    let mut mark_stack: Vec<String> = Vec::new();
    // For links/images: href while inside a Link or Image tag
    let mut link_href: Option<String> = None;
    // Whether we're currently inside a table header section
    let mut in_table_head = false;
    // Raw markup of the HTML block being read (pulldown-cmark emits it line by line)
    let mut html_block = String::new();
    // Counter for ADF localId values required by taskList/taskItem nodes
    let mut local_id: u32 = 0;
    let mut next_id = || {
        local_id += 1;
        local_id.to_string()
    };

    let link_mark = |href: &str| json!({ "type": "link", "attrs": { "href": href } });

    // Append a finished block node to its enclosing block, or to the document root.
    fn push_block(
        node: Value,
        block_stack: &mut [(String, Vec<Value>)],
        doc_content: &mut Vec<Value>,
    ) {
        match block_stack.last_mut() {
            Some((_, parent)) => parent.push(node),
            None => doc_content.push(node),
        }
    }

    // Flush the inline buffer as content into the nearest enclosing block.
    // If no block is open, wraps in a paragraph and pushes to doc_content.
    let flush_inline = |inline_buf: &mut Vec<Value>,
                        block_stack: &mut Vec<(String, Vec<Value>)>,
                        doc_content: &mut Vec<Value>| {
        if inline_buf.is_empty() {
            return;
        }
        let nodes = std::mem::take(inline_buf);
        if let Some((_, content)) = block_stack.last_mut() {
            content.extend(nodes);
        } else {
            // Bare inline content — wrap in a paragraph
            doc_content.push(json!({ "type": "paragraph", "content": nodes }));
        }
    };

    // Tight list items carry their text without a Paragraph event, but ADF
    // listItem children must be blocks: wrap pending text in a paragraph.
    let flush_list_item_inline_as_paragraph =
        |inline_buf: &mut Vec<Value>, block_stack: &mut Vec<(String, Vec<Value>)>| {
            if inline_buf.is_empty() {
                return;
            }

            if let Some((key, content)) = block_stack.last_mut()
                && key == "listItem"
            {
                let nodes = std::mem::take(inline_buf);
                content.push(json!({ "type": "paragraph", "content": nodes }));
            }
        };

    for event in parser {
        // A list item's leading text must precede any nested block it contains.
        if let Event::Start(
            Tag::Paragraph
            | Tag::Heading { .. }
            | Tag::BlockQuote(_)
            | Tag::CodeBlock(_)
            | Tag::HtmlBlock
            | Tag::List(_)
            | Tag::Table(_),
        ) = &event
        {
            flush_list_item_inline_as_paragraph(&mut inline_buf, &mut block_stack);
        }

        match event {
            // ── Block opens ──────────────────────────────────────────────
            Event::Start(Tag::Heading { level, .. }) => {
                let lvl = match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                };
                block_stack.push((format!("heading:{lvl}"), Vec::new()));
            }
            Event::End(TagEnd::Heading(_)) => {
                flush_inline(&mut inline_buf, &mut block_stack, &mut doc_content);
                if let Some((key, content)) = block_stack.pop() {
                    let level: u64 = key.split(':').nth(1).unwrap_or("1").parse().unwrap_or(1);
                    let node = json!({
                        "type": "heading",
                        "attrs": { "level": level },
                        "content": content
                    });
                    push_block(node, &mut block_stack, &mut doc_content);
                }
            }

            Event::Start(Tag::Paragraph) => {
                block_stack.push(("paragraph".to_string(), Vec::new()));
            }
            Event::End(TagEnd::Paragraph) => {
                flush_inline(&mut inline_buf, &mut block_stack, &mut doc_content);
                if let Some((_, content)) = block_stack.pop() {
                    let node = json!({ "type": "paragraph", "content": content });
                    push_block(node, &mut block_stack, &mut doc_content);
                }
            }

            Event::Start(Tag::BlockQuote(_)) => {
                block_stack.push(("blockquote".to_string(), Vec::new()));
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                if let Some((_, content)) = block_stack.pop() {
                    let node = json!({ "type": "blockquote", "content": content });
                    push_block(node, &mut block_stack, &mut doc_content);
                }
            }

            Event::Start(Tag::List(None)) => {
                block_stack.push(("bulletList".to_string(), Vec::new()));
            }
            Event::End(TagEnd::List(false)) => {
                if let Some((key, content)) = block_stack.pop() {
                    let node = if key == "taskList" {
                        json!({ "type": "taskList", "attrs": { "localId": next_id() }, "content": content })
                    } else {
                        json!({ "type": "bulletList", "content": content })
                    };
                    push_block(node, &mut block_stack, &mut doc_content);
                }
            }

            Event::Start(Tag::List(Some(start))) => {
                block_stack.push((format!("orderedList:{start}"), Vec::new()));
            }
            Event::End(TagEnd::List(true)) => {
                if let Some((key, content)) = block_stack.pop() {
                    let order: u64 = key.split(':').nth(1).unwrap_or("1").parse().unwrap_or(1);
                    let node = json!({
                        "type": "orderedList",
                        "attrs": { "order": order },
                        "content": content
                    });
                    push_block(node, &mut block_stack, &mut doc_content);
                }
            }

            Event::Start(Tag::Item) => {
                block_stack.push(("listItem".to_string(), Vec::new()));
            }
            Event::End(TagEnd::Item) => {
                let is_task_item = block_stack
                    .last()
                    .is_some_and(|(key, _)| key.starts_with("taskItem:"));
                if is_task_item {
                    // ADF taskItem takes inline content directly — no paragraph wrapper.
                    flush_inline(&mut inline_buf, &mut block_stack, &mut doc_content);
                } else {
                    flush_list_item_inline_as_paragraph(&mut inline_buf, &mut block_stack);
                }
                if let Some((key, content)) = block_stack.pop() {
                    let node = match key.strip_prefix("taskItem:") {
                        Some(state) => json!({
                            "type": "taskItem",
                            "attrs": { "localId": next_id(), "state": state },
                            "content": content
                        }),
                        None => json!({ "type": "listItem", "content": content }),
                    };
                    push_block(node, &mut block_stack, &mut doc_content);
                }
            }

            Event::Start(Tag::CodeBlock(kind)) => {
                let lang = match kind {
                    pulldown_cmark::CodeBlockKind::Fenced(lang) => lang.to_string(),
                    pulldown_cmark::CodeBlockKind::Indented => String::new(),
                };
                block_stack.push((format!("codeBlock:{lang}"), Vec::new()));
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some((key, content)) = block_stack.pop() {
                    let lang = key.split_once(':').map(|x| x.1).unwrap_or("").to_string();
                    let node = if lang.is_empty() {
                        json!({ "type": "codeBlock", "content": content })
                    } else {
                        json!({
                            "type": "codeBlock",
                            "attrs": { "language": lang },
                            "content": content
                        })
                    };
                    push_block(node, &mut block_stack, &mut doc_content);
                }
            }

            // ── Inline opens ─────────────────────────────────────────────
            Event::Start(Tag::Strong) => mark_stack.push("strong".to_string()),
            Event::End(TagEnd::Strong) => {
                mark_stack.retain(|m| m != "strong");
            }

            Event::Start(Tag::Emphasis) => mark_stack.push("em".to_string()),
            Event::End(TagEnd::Emphasis) => {
                mark_stack.retain(|m| m != "em");
            }

            Event::Start(Tag::Strikethrough) => mark_stack.push("strike".to_string()),
            Event::End(TagEnd::Strikethrough) => {
                mark_stack.retain(|m| m != "strike");
            }

            Event::Start(Tag::Link { dest_url, .. }) => {
                link_href = Some(dest_url.to_string());
                mark_stack.push("link".to_string());
            }
            Event::End(TagEnd::Link) => {
                mark_stack.retain(|m| m != "link");
                link_href = None;
            }

            // Images: no ADF media upload path, render alt text as a link
            Event::Start(Tag::Image { dest_url, .. }) => {
                link_href = Some(dest_url.to_string());
                mark_stack.push("link".to_string());
            }
            Event::End(TagEnd::Image) => {
                mark_stack.retain(|m| m != "link");
                link_href = None;
            }

            // ── Task list marker ─────────────────────────────────────────
            Event::TaskListMarker(checked) => {
                let state = if checked { "DONE" } else { "TODO" };
                // Promote current listItem → taskItem
                for entry in block_stack.iter_mut().rev() {
                    if entry.0 == "listItem" {
                        entry.0 = format!("taskItem:{state}");
                        break;
                    }
                }
                // Promote enclosing bulletList → taskList
                for entry in block_stack.iter_mut().rev() {
                    if entry.0 == "bulletList" {
                        entry.0 = "taskList".to_string();
                        break;
                    }
                }
            }

            // ── Table blocks ─────────────────────────────────────────────
            Event::Start(Tag::Table(_)) => {
                block_stack.push(("table".to_string(), Vec::new()));
            }
            Event::End(TagEnd::Table) => {
                if let Some((_, content)) = block_stack.pop() {
                    let node = json!({ "type": "table", "content": content });
                    push_block(node, &mut block_stack, &mut doc_content);
                }
            }

            // TableHead implicitly represents a single header row (no separate
            // TableRow events are emitted inside it), so we push a row wrapper here.
            Event::Start(Tag::TableHead) => {
                in_table_head = true;
                block_stack.push(("tableRow".to_string(), Vec::new()));
            }
            Event::End(TagEnd::TableHead) => {
                in_table_head = false;
                if let Some((_, content)) = block_stack.pop() {
                    let node = json!({ "type": "tableRow", "content": content });
                    push_block(node, &mut block_stack, &mut doc_content);
                }
            }

            // Body rows do emit explicit TableRow events.
            Event::Start(Tag::TableRow) => {
                block_stack.push(("tableRow".to_string(), Vec::new()));
            }
            Event::End(TagEnd::TableRow) => {
                if let Some((_, content)) = block_stack.pop() {
                    let node = json!({ "type": "tableRow", "content": content });
                    push_block(node, &mut block_stack, &mut doc_content);
                }
            }

            Event::Start(Tag::TableCell) => {
                let cell_type = if in_table_head {
                    "tableHeader"
                } else {
                    "tableCell"
                };
                block_stack.push((cell_type.to_string(), Vec::new()));
            }
            Event::End(TagEnd::TableCell) => {
                flush_inline(&mut inline_buf, &mut block_stack, &mut doc_content);
                if let Some((key, content)) = block_stack.pop() {
                    // ADF table cells require block-level children; wrap inline nodes
                    let is_inline = content
                        .first()
                        .and_then(|n| n.get("type"))
                        .and_then(|t| t.as_str())
                        .map(|t| t == "text" || t == "hardBreak")
                        .unwrap_or(false);
                    let cell_content = if is_inline || content.is_empty() {
                        vec![json!({ "type": "paragraph", "content": content })]
                    } else {
                        content
                    };
                    let node = json!({ "type": key, "attrs": {}, "content": cell_content });
                    push_block(node, &mut block_stack, &mut doc_content);
                }
            }

            // ── Leaf events ───────────────────────────────────────────────
            Event::Text(t) => {
                let marks: Vec<Value> = mark_stack
                    .iter()
                    .map(|m| {
                        if m == "link" {
                            link_mark(link_href.as_deref().unwrap_or(""))
                        } else {
                            json!({ "type": m })
                        }
                    })
                    .collect();

                let node = if marks.is_empty() {
                    json!({ "type": "text", "text": t.as_ref() })
                } else {
                    json!({ "type": "text", "text": t.as_ref(), "marks": marks })
                };

                // Code blocks collect text directly into the block stack
                if block_stack
                    .last()
                    .map(|(k, _)| k.starts_with("codeBlock"))
                    .unwrap_or(false)
                {
                    if let Some((_, content)) = block_stack.last_mut() {
                        content.push(node);
                    }
                } else {
                    inline_buf.push(node);
                }
            }

            Event::Code(t) => {
                // ADF lets the code mark combine with a link, but no other mark.
                let mut marks = vec![json!({ "type": "code" })];
                if let Some(href) = &link_href {
                    marks.push(link_mark(href));
                }
                inline_buf.push(json!({ "type": "text", "text": t.as_ref(), "marks": marks }));
            }

            Event::SoftBreak => {
                // Soft breaks become a space to avoid word-joining
                inline_buf.push(json!({ "type": "text", "text": " " }));
            }

            Event::HardBreak => {
                inline_buf.push(json!({ "type": "hardBreak" }));
            }

            Event::Rule => {
                flush_list_item_inline_as_paragraph(&mut inline_buf, &mut block_stack);
                flush_inline(&mut inline_buf, &mut block_stack, &mut doc_content);
                doc_content.push(json!({ "type": "rule" }));
            }

            // Inline HTML tags (<b>, <br> etc.): pulldown-cmark surfaces the
            // text content between them via normal Text events, so the tag
            // tokens themselves can be silently ignored.
            Event::InlineHtml(_) => {}

            // Block HTML has no ADF equivalent: strip the tags and keep the
            // remaining text as its own paragraph so it is not silently lost.
            Event::Html(raw) => html_block.push_str(&raw),
            Event::End(TagEnd::HtmlBlock) => {
                let html = std::mem::take(&mut html_block);
                let text = strip_html_tags(&html)
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                if !text.is_empty() {
                    let node = json!({
                        "type": "paragraph",
                        "content": [{ "type": "text", "text": text }]
                    });
                    push_block(node, &mut block_stack, &mut doc_content);
                }
            }

            _ => {}
        }
    }

    // Flush any trailing inline content
    flush_inline(&mut inline_buf, &mut block_stack, &mut doc_content);

    json!({ "type": "doc", "version": 1, "content": doc_content })
}

/// Extract plain text from ADF document
pub(crate) fn adf_to_text(adf: &serde_json::Value) -> String {
    use serde_json::{Map, Value};

    fn attr<'a>(node: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
        node.get("attrs")?.get(key)?.as_str()
    }

    fn extract_text(value: &Value) -> String {
        match value {
            Value::Object(obj) => {
                let node_type = obj.get("type").and_then(|t| t.as_str()).unwrap_or("");

                match node_type {
                    "text" => obj
                        .get("text")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string(),
                    "hardBreak" => "\n".to_string(),
                    // Nodes whose visible text lives in attrs rather than in
                    // content. Jira Cloud turns every pasted URL into a card.
                    "inlineCard" => attr(obj, "url").unwrap_or_default().to_string(),
                    "blockCard" | "embedCard" => {
                        format!("{}\n\n", attr(obj, "url").unwrap_or_default())
                    }
                    // A mention's text already includes the leading '@'.
                    "mention" | "status" => attr(obj, "text").unwrap_or_default().to_string(),
                    "emoji" => attr(obj, "text")
                        .or_else(|| attr(obj, "shortName"))
                        .unwrap_or_default()
                        .to_string(),
                    // ADF dates are UTC midnight timestamps in milliseconds.
                    "date" => attr(obj, "timestamp")
                        .and_then(|ms| ms.parse().ok())
                        .and_then(chrono::DateTime::from_timestamp_millis)
                        .map(|date| date.format("%Y-%m-%d").to_string())
                        .unwrap_or_default(),
                    "paragraph" | "heading" | "codeBlock" | "blockquote" => {
                        let inner = obj.get("content").map(extract_text).unwrap_or_default();
                        format!("{}\n\n", inner.trim())
                    }
                    "listItem" | "taskItem" => {
                        let inner = obj.get("content").map(extract_text).unwrap_or_default();
                        format!("- {}\n", inner.trim())
                    }
                    "bulletList" | "orderedList" | "taskList" => {
                        let inner = obj.get("content").map(extract_text).unwrap_or_default();
                        format!("{}\n", inner.trim())
                    }
                    "tableRow" => {
                        let inner = obj.get("content").map(extract_text).unwrap_or_default();
                        format!("{}\n", inner.trim())
                    }
                    "tableHeader" | "tableCell" => {
                        let inner = obj.get("content").map(extract_text).unwrap_or_default();
                        format!("| {} ", inner.trim())
                    }
                    _ => {
                        if let Some(content) = obj.get("content") {
                            extract_text(content)
                        } else {
                            String::new()
                        }
                    }
                }
            }
            Value::Array(arr) => arr.iter().map(extract_text).collect::<Vec<_>>().join(""),
            _ => String::new(),
        }
    }
    extract_text(adf).trim().to_string()
}

#[cfg(test)]
mod markdown_to_adf_tests {
    use super::*;

    fn first_block(adf: &serde_json::Value) -> &serde_json::Value {
        &adf["content"][0]
    }

    #[test]
    fn plain_text_produces_paragraph() {
        let adf = markdown_to_adf("Hello world");
        let block = first_block(&adf);
        assert_eq!(block["type"], "paragraph");
        assert_eq!(block["content"][0]["text"], "Hello world");
    }

    #[test]
    fn heading_level_1() {
        let adf = markdown_to_adf("# Title");
        let block = first_block(&adf);
        assert_eq!(block["type"], "heading");
        assert_eq!(block["attrs"]["level"], 1);
        assert_eq!(block["content"][0]["text"], "Title");
    }

    #[test]
    fn heading_level_3() {
        let adf = markdown_to_adf("### Sub");
        let block = first_block(&adf);
        assert_eq!(block["type"], "heading");
        assert_eq!(block["attrs"]["level"], 3);
    }

    #[test]
    fn bold_text() {
        let adf = markdown_to_adf("**bold**");
        let block = first_block(&adf);
        let node = &block["content"][0];
        assert_eq!(node["text"], "bold");
        assert_eq!(node["marks"][0]["type"], "strong");
    }

    #[test]
    fn italic_text() {
        let adf = markdown_to_adf("*italic*");
        let block = first_block(&adf);
        let node = &block["content"][0];
        assert_eq!(node["text"], "italic");
        assert_eq!(node["marks"][0]["type"], "em");
    }

    #[test]
    fn inline_code() {
        let adf = markdown_to_adf("`code`");
        let block = first_block(&adf);
        let node = &block["content"][0];
        assert_eq!(node["text"], "code");
        assert_eq!(node["marks"][0]["type"], "code");
    }

    #[test]
    fn fenced_code_block_with_language() {
        let adf = markdown_to_adf("```rust\nfn main() {}\n```");
        let block = first_block(&adf);
        assert_eq!(block["type"], "codeBlock");
        assert_eq!(block["attrs"]["language"], "rust");
        assert_eq!(block["content"][0]["text"], "fn main() {}\n");
    }

    #[test]
    fn bullet_list() {
        let adf = markdown_to_adf("- one\n- two");
        let block = first_block(&adf);
        assert_eq!(block["type"], "bulletList");
        assert_eq!(block["content"][0]["type"], "listItem");
    }

    #[test]
    fn ordered_list() {
        let adf = markdown_to_adf("1. first\n2. second");
        let block = first_block(&adf);
        assert_eq!(block["type"], "orderedList");
        assert_eq!(block["content"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn link() {
        let adf = markdown_to_adf("[Jira](https://jira.example.com)");
        let block = first_block(&adf);
        let node = &block["content"][0];
        assert_eq!(node["text"], "Jira");
        assert_eq!(node["marks"][0]["type"], "link");
        assert_eq!(
            node["marks"][0]["attrs"]["href"],
            "https://jira.example.com"
        );
    }

    #[test]
    fn horizontal_rule() {
        let adf = markdown_to_adf("---");
        let block = first_block(&adf);
        assert_eq!(block["type"], "rule");
    }

    #[test]
    fn doc_structure() {
        let adf = markdown_to_adf("test");
        assert_eq!(adf["type"], "doc");
        assert_eq!(adf["version"], 1);
        assert!(adf["content"].is_array());
    }

    #[test]
    fn task_list_unchecked() {
        let adf = markdown_to_adf("- [ ] todo item");
        let block = first_block(&adf);
        assert_eq!(block["type"], "taskList");
        let item = &block["content"][0];
        assert_eq!(item["type"], "taskItem");
        assert_eq!(item["attrs"]["state"], "TODO");
    }

    #[test]
    fn task_list_checked() {
        let adf = markdown_to_adf("- [x] done item");
        let block = first_block(&adf);
        assert_eq!(block["type"], "taskList");
        let item = &block["content"][0];
        assert_eq!(item["type"], "taskItem");
        assert_eq!(item["attrs"]["state"], "DONE");
    }

    #[test]
    fn mixed_task_list() {
        let adf = markdown_to_adf("- [ ] pending\n- [x] done");
        let block = first_block(&adf);
        assert_eq!(block["type"], "taskList");
        assert_eq!(block["content"][0]["attrs"]["state"], "TODO");
        assert_eq!(block["content"][1]["attrs"]["state"], "DONE");
    }

    #[test]
    fn table_basic() {
        let md = "| A | B |\n|---|---|\n| 1 | 2 |";
        let adf = markdown_to_adf(md);
        let block = first_block(&adf);
        assert_eq!(block["type"], "table");
        // Header row
        let header_row = &block["content"][0];
        assert_eq!(header_row["type"], "tableRow");
        assert_eq!(header_row["content"][0]["type"], "tableHeader");
        // Body row
        let body_row = &block["content"][1];
        assert_eq!(body_row["type"], "tableRow");
        assert_eq!(body_row["content"][0]["type"], "tableCell");
    }

    #[test]
    fn table_cell_content_wrapped_in_paragraph() {
        let md = "| text |\n|---|\n| data |";
        let adf = markdown_to_adf(md);
        // table -> tableRow -> tableHeader -> paragraph
        let header_row = &adf["content"][0]["content"][0];
        let header_cell = &header_row["content"][0];
        assert_eq!(header_cell["type"], "tableHeader");
        // Cell content must be a paragraph (ADF requirement)
        assert_eq!(header_cell["content"][0]["type"], "paragraph");
    }

    #[test]
    fn image_renders_as_link() {
        let adf = markdown_to_adf("![alt](https://example.com/img.png)");
        let block = first_block(&adf);
        let node = &block["content"][0];
        assert_eq!(node["text"], "alt");
        assert_eq!(node["marks"][0]["type"], "link");
        assert_eq!(
            node["marks"][0]["attrs"]["href"],
            "https://example.com/img.png"
        );
    }

    #[test]
    fn strikethrough() {
        let adf = markdown_to_adf("~~deleted~~");
        let block = first_block(&adf);
        let node = &block["content"][0];
        assert_eq!(node["text"], "deleted");
        assert_eq!(node["marks"][0]["type"], "strike");
    }

    #[test]
    fn nested_lists() {
        let md = "- outer\n  - inner";
        let adf = markdown_to_adf(md);
        let outer = first_block(&adf);
        assert_eq!(outer["type"], "bulletList");
        // The outer list item should contain a nested bulletList
        let outer_item = &outer["content"][0];
        assert_eq!(outer_item["type"], "listItem");
        let nested = outer_item["content"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["type"] == "bulletList")
            .expect("nested bulletList not found");
        assert_eq!(nested["content"][0]["type"], "listItem");
    }

    #[test]
    fn nested_bullet_list_preserves_parent_text_as_paragraph() {
        // Arrange
        let md = "- Re-verify consumers:\n  - `grep -rn \"foo\" src/`\n  - `grep -rn \"bar\" src/`";

        // Act
        let adf = markdown_to_adf(md);

        // Assert
        let outer_item = &first_block(&adf)["content"][0];
        assert_eq!(outer_item["type"], "listItem");
        assert_eq!(outer_item["content"][0]["type"], "paragraph");
        assert_eq!(
            outer_item["content"][0]["content"][0]["text"],
            "Re-verify consumers:"
        );
        assert_eq!(outer_item["content"][1]["type"], "bulletList");

        let child_item = &outer_item["content"][1]["content"][0];
        let child_paragraph = &child_item["content"][0];
        assert_eq!(child_paragraph["type"], "paragraph");
        assert_eq!(
            child_paragraph["content"][0]["text"],
            "grep -rn \"foo\" src/"
        );
        assert_eq!(child_paragraph["content"][0]["marks"][0]["type"], "code");

        let child_text = child_paragraph["content"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|node| node["text"].as_str())
            .collect::<String>();
        assert!(
            !child_text.contains("Re-verify consumers:"),
            "child item text should not contain parent item text"
        );
    }

    #[test]
    fn nested_ordered_list_preserves_parent_text_as_paragraph() {
        // Arrange
        let md = "1. Prepare release:\n   1. Build artifacts\n   2. Upload artifacts";

        // Act
        let adf = markdown_to_adf(md);

        // Assert
        let outer_item = &first_block(&adf)["content"][0];
        assert_eq!(outer_item["type"], "listItem");
        assert_eq!(outer_item["content"][0]["type"], "paragraph");
        assert_eq!(
            outer_item["content"][0]["content"][0]["text"],
            "Prepare release:"
        );
        assert_eq!(outer_item["content"][1]["type"], "orderedList");

        let child_paragraph = &outer_item["content"][1]["content"][0]["content"][0];
        assert_eq!(child_paragraph["type"], "paragraph");
        assert_eq!(child_paragraph["content"][0]["text"], "Build artifacts");
    }

    #[test]
    fn nested_mixed_list_preserves_parent_text_as_paragraph() {
        // Arrange
        let md = "- Release checklist:\n  1. Build artifacts\n  2. Upload artifacts";

        // Act
        let adf = markdown_to_adf(md);

        // Assert
        let outer_item = &first_block(&adf)["content"][0];
        assert_eq!(outer_item["type"], "listItem");
        assert_eq!(outer_item["content"][0]["type"], "paragraph");
        assert_eq!(
            outer_item["content"][0]["content"][0]["text"],
            "Release checklist:"
        );
        assert_eq!(outer_item["content"][1]["type"], "orderedList");

        let child_paragraph = &outer_item["content"][1]["content"][0]["content"][0];
        assert_eq!(child_paragraph["type"], "paragraph");
        assert_eq!(child_paragraph["content"][0]["text"], "Build artifacts");
    }

    #[test]
    fn multi_paragraph_list_item() {
        // Two paragraphs inside one list item (separated by a blank line + indent)
        let md = "- First paragraph.\n\n  Second paragraph.";
        let adf = markdown_to_adf(md);
        let list = first_block(&adf);
        assert_eq!(list["type"], "bulletList");
        let item = &list["content"][0];
        assert_eq!(item["type"], "listItem");
        // Item must contain two paragraphs
        let paragraphs: Vec<_> = item["content"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|n| n["type"] == "paragraph")
            .collect();
        assert_eq!(paragraphs.len(), 2);
    }

    #[test]
    fn nested_blockquotes() {
        let md = "> outer\n>\n> > inner";
        let adf = markdown_to_adf(md);
        let outer = first_block(&adf);
        assert_eq!(outer["type"], "blockquote");
        // The outer blockquote should contain a nested blockquote
        let inner = outer["content"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["type"] == "blockquote")
            .expect("nested blockquote not found");
        assert_eq!(inner["type"], "blockquote");
    }

    #[test]
    fn mixed_inline_marks() {
        // Bold wrapping italic: **_both_**
        let adf = markdown_to_adf("**_both_**");
        let block = first_block(&adf);
        let node = &block["content"][0];
        assert_eq!(node["text"], "both");
        let marks: Vec<&str> = node["marks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["type"].as_str().unwrap())
            .collect();
        assert!(marks.contains(&"strong"), "expected strong mark");
        assert!(marks.contains(&"em"), "expected em mark");
    }

    #[test]
    fn inline_html_text_preserved() {
        // Inline tags are stripped; inner text comes through Text events
        let adf = markdown_to_adf("Hello <b>world</b>");
        let block = first_block(&adf);
        let text: String = block["content"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|n| n["text"].as_str())
            .collect();
        assert!(text.contains("Hello"), "expected 'Hello' in output");
        assert!(text.contains("world"), "expected 'world' in output");
    }

    #[test]
    fn block_html_text_preserved() {
        // Block-level HTML: tags stripped, inner text preserved
        let adf = markdown_to_adf("<div>block content</div>");
        let all_text: String = adf["content"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|block| block["content"].as_array())
            .flatten()
            .filter_map(|n| n["text"].as_str())
            .collect();
        assert!(
            all_text.contains("block content"),
            "expected text from block HTML"
        );
    }

    #[test]
    fn block_html_stays_out_of_following_paragraph() {
        let adf = markdown_to_adf("<div>block content</div>\n\nNext paragraph");
        let blocks = adf["content"].as_array().unwrap();
        assert_eq!(blocks.len(), 2, "got: {adf}");
        assert_eq!(blocks[0]["content"][0]["text"], "block content");
        assert_eq!(blocks[1]["content"][0]["text"], "Next paragraph");
    }

    #[test]
    fn inline_code_link_keeps_link_mark() {
        let adf = markdown_to_adf("[`parse()`](https://docs.example.com/parse)");
        let node = &first_block(&adf)["content"][0];
        assert_eq!(node["text"], "parse()");
        assert_eq!(
            node["marks"],
            serde_json::json!([
                { "type": "code" },
                { "type": "link", "attrs": { "href": "https://docs.example.com/parse" } }
            ])
        );
    }

    #[test]
    fn tight_list_item_text_precedes_nested_code_block() {
        let adf = markdown_to_adf("- Run:\n  ```sh\n  make\n  ```");
        let item = &first_block(&adf)["content"][0];
        assert_eq!(item["type"], "listItem");
        assert_eq!(item["content"][0]["type"], "paragraph");
        assert_eq!(item["content"][0]["content"][0]["text"], "Run:");
        assert_eq!(item["content"][1]["type"], "codeBlock");
        assert_eq!(item["content"].as_array().unwrap().len(), 2);
    }
}

#[cfg(test)]
mod adf_to_text_tests {
    use super::*;

    #[test]
    fn renders_text_carried_in_inline_node_attrs() {
        let adf = serde_json::json!({
            "type": "doc",
            "version": 1,
            "content": [{
                "type": "paragraph",
                "content": [
                    { "type": "mention", "attrs": { "id": "557058:abc", "text": "@Jane Doe" } },
                    { "type": "text", "text": " see " },
                    { "type": "inlineCard", "attrs": { "url": "https://example.atlassian.net/browse/PROJ-1" } },
                    { "type": "text", "text": " " },
                    { "type": "emoji", "attrs": { "shortName": ":smile:", "text": "😄" } },
                    { "type": "text", "text": " " },
                    { "type": "status", "attrs": { "text": "IN REVIEW", "color": "blue" } },
                    { "type": "text", "text": " by " },
                    { "type": "date", "attrs": { "timestamp": "1704067200000" } }
                ]
            }]
        });

        assert_eq!(
            adf_to_text(&adf),
            "@Jane Doe see https://example.atlassian.net/browse/PROJ-1 😄 IN REVIEW by 2024-01-01"
        );
    }
}
