use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::comment::JiraComment;
use super::project::JiraProjectRef;
use super::user::JiraUser;

/// Jira issue
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraIssue {
    /// Internal numeric ID
    pub id: String,
    /// Issue key (e.g., "PROJ-123")
    pub key: String,
    /// Self URL
    #[serde(rename = "self")]
    pub self_url: Option<String>,
    /// Issue fields
    pub fields: JiraIssueFields,
}

/// Issue fields container
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraIssueFields {
    /// Issue summary/title
    pub summary: String,
    /// Issue description in ADF format
    pub description: Option<serde_json::Value>,
    /// Issue status
    pub status: JiraStatus,
    /// Issue priority
    pub priority: Option<JiraPriority>,
    /// Issue type
    pub issuetype: JiraIssueType,
    /// Project reference
    pub project: JiraProjectRef,
    /// Assignee
    pub assignee: Option<JiraUser>,
    /// Reporter
    pub reporter: Option<JiraUser>,
    /// Labels (equivalent to tags)
    #[serde(default)]
    pub labels: Vec<String>,
    /// Creation timestamp
    pub created: Option<String>,
    /// Last update timestamp
    pub updated: Option<String>,
    /// Subtasks
    #[serde(default)]
    pub subtasks: Vec<JiraIssueRef>,
    /// Parent issue (if this is a subtask)
    pub parent: Option<JiraIssueRef>,
    /// Issue links
    #[serde(default)]
    pub issuelinks: Vec<JiraIssueLink>,
    /// Comments (only included when expanded)
    pub comment: Option<JiraCommentsContainer>,
    /// Attachments on this issue.
    #[serde(default)]
    pub attachment: Vec<JiraAttachment>,
    /// Extra/custom fields not captured by the named fields above.
    /// Keys are field IDs like "customfield_10016".
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Comments container in issue response
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraCommentsContainer {
    pub comments: Vec<JiraComment>,
    #[serde(default)]
    pub total: usize,
}

/// Jira issue attachment.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraAttachment {
    pub id: String,
    pub filename: String,
    #[serde(default)]
    pub size: i64,
    #[serde(default)]
    pub mime_type: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub created: Option<String>,
    #[serde(default)]
    pub author: Option<JiraUser>,
}

/// Issue reference (used in subtasks, parent, links)
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraIssueRef {
    /// Internal ID
    pub id: String,
    /// Issue key
    pub key: String,
    /// Self URL
    #[serde(rename = "self")]
    pub self_url: Option<String>,
    /// Summary (sometimes included)
    pub fields: Option<JiraIssueRefFields>,
}

/// Minimal fields in issue reference
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraIssueRefFields {
    pub summary: Option<String>,
    pub status: Option<JiraStatus>,
    pub priority: Option<JiraPriority>,
    pub issuetype: Option<JiraIssueType>,
}

/// Issue status
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraStatus {
    /// Status ID
    pub id: Option<String>,
    /// Status name
    pub name: String,
    /// Status category
    pub status_category: Option<JiraStatusCategory>,
}

/// Status category (used to determine if issue is resolved)
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraStatusCategory {
    /// Category key (e.g., "done", "indeterminate", "new")
    pub key: String,
    /// Category name
    pub name: Option<String>,
}

/// Issue priority
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraPriority {
    /// Priority ID
    pub id: Option<String>,
    /// Priority name
    pub name: String,
}

/// Issue type
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraIssueType {
    /// Type ID
    pub id: Option<String>,
    /// Type name
    pub name: String,
    /// Whether this is a subtask type
    #[serde(default)]
    pub subtask: bool,
}

/// Issue link
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraIssueLink {
    /// Link ID
    pub id: Option<String>,
    /// Link type
    #[serde(rename = "type")]
    pub link_type: JiraIssueLinkType,
    /// Inward issue (if this link points inward)
    pub inward_issue: Option<JiraIssueRef>,
    /// Outward issue (if this link points outward)
    pub outward_issue: Option<JiraIssueRef>,
}

/// Issue link type
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraIssueLinkType {
    /// Type ID
    pub id: Option<String>,
    /// Type name
    pub name: String,
    /// Inward description (e.g., "is blocked by")
    pub inward: Option<String>,
    /// Outward description (e.g., "blocks")
    pub outward: Option<String>,
}

/// Search result response from `/search/jql` endpoint.
///
/// The new Jira Cloud search endpoint returns `isLast` instead of `total`.
/// It does not provide a total count of matching issues.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraSearchResult {
    /// Issues in this page
    pub issues: Vec<JiraIssue>,
    /// Whether this is the last page of results
    #[serde(default)]
    pub is_last: bool,
}

/// Request to create an issue
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateJiraIssue {
    pub fields: CreateJiraIssueFields,
}

/// Fields for issue creation
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateJiraIssueFields {
    /// Project (key or id)
    pub project: ProjectId,
    /// Summary
    pub summary: String,
    /// Description in ADF format
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<serde_json::Value>,
    /// Issue type
    pub issuetype: IssueTypeId,
    /// Priority
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<PriorityId>,
    /// Labels
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
    /// Parent issue (for subtasks)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<ParentId>,
    /// Arbitrary custom fields (e.g., "customfield_10016": 5)
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

/// Project identifier for requests
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectId {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

/// Issue type identifier for requests
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueTypeId {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Priority identifier for requests
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PriorityId {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Parent issue identifier for requests
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParentId {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

/// Request to update an issue
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateJiraIssue {
    pub fields: UpdateJiraIssueFields,
}

/// Fields for issue update
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateJiraIssueFields {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<PriorityId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<ParentId>,
    /// Arbitrary custom fields (e.g., "customfield_10016": 5)
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

/// Request to create an issue link
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateJiraIssueLink {
    #[serde(rename = "type")]
    pub link_type: IssueLinkTypeName,
    pub inward_issue: IssueKeyRef,
    pub outward_issue: IssueKeyRef,
}

/// Issue link type name for requests
#[derive(Debug, Clone, Serialize)]
pub struct IssueLinkTypeName {
    pub name: String,
}

/// Issue key reference for requests
#[derive(Debug, Clone, Serialize)]
pub struct IssueKeyRef {
    pub key: String,
}

/// Request for JQL search
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraSearchRequest {
    pub jql: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_at: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_results: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expand: Option<Vec<String>>,
}

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
pub fn markdown_to_adf(text: &str) -> serde_json::Value {
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
    // Counter for ADF localId values required by taskList/taskItem nodes
    let mut local_id: u32 = 0;
    let mut next_id = || {
        local_id += 1;
        local_id.to_string()
    };

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
                    if let Some((_, parent)) = block_stack.last_mut() {
                        parent.push(node);
                    } else {
                        doc_content.push(node);
                    }
                }
            }

            Event::Start(Tag::Paragraph) => {
                block_stack.push(("paragraph".to_string(), Vec::new()));
            }
            Event::End(TagEnd::Paragraph) => {
                flush_inline(&mut inline_buf, &mut block_stack, &mut doc_content);
                if let Some((_, content)) = block_stack.pop() {
                    let node = json!({ "type": "paragraph", "content": content });
                    if let Some((_, parent)) = block_stack.last_mut() {
                        parent.push(node);
                    } else {
                        doc_content.push(node);
                    }
                }
            }

            Event::Start(Tag::BlockQuote(_)) => {
                block_stack.push(("blockquote".to_string(), Vec::new()));
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                if let Some((_, content)) = block_stack.pop() {
                    let node = json!({ "type": "blockquote", "content": content });
                    if let Some((_, parent)) = block_stack.last_mut() {
                        parent.push(node);
                    } else {
                        doc_content.push(node);
                    }
                }
            }

            Event::Start(Tag::List(None)) => {
                flush_list_item_inline_as_paragraph(&mut inline_buf, &mut block_stack);
                block_stack.push(("bulletList".to_string(), Vec::new()));
            }
            Event::End(TagEnd::List(false)) => {
                if let Some((key, content)) = block_stack.pop() {
                    let node = if key == "taskList" {
                        json!({ "type": "taskList", "attrs": { "localId": next_id() }, "content": content })
                    } else {
                        json!({ "type": "bulletList", "content": content })
                    };
                    if let Some((_, parent)) = block_stack.last_mut() {
                        parent.push(node);
                    } else {
                        doc_content.push(node);
                    }
                }
            }

            Event::Start(Tag::List(Some(start))) => {
                flush_list_item_inline_as_paragraph(&mut inline_buf, &mut block_stack);
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
                    if let Some((_, parent)) = block_stack.last_mut() {
                        parent.push(node);
                    } else {
                        doc_content.push(node);
                    }
                }
            }

            Event::Start(Tag::Item) => {
                block_stack.push(("listItem".to_string(), Vec::new()));
            }
            Event::End(TagEnd::Item) => {
                flush_inline(&mut inline_buf, &mut block_stack, &mut doc_content);
                if let Some((key, content)) = block_stack.pop() {
                    let node = if key.starts_with("taskItem:") {
                        // ADF taskItem takes inline content directly — no paragraph wrapper.
                        let state = key.split_once(':').map(|x| x.1).unwrap_or("TODO");
                        json!({
                            "type": "taskItem",
                            "attrs": { "localId": next_id(), "state": state },
                            "content": content
                        })
                    } else {
                        // ADF listItem requires block-level children; wrap bare inline nodes.
                        let is_inline = content
                            .first()
                            .and_then(|n| n.get("type"))
                            .and_then(|t| t.as_str())
                            .map(|t| t == "text" || t == "hardBreak")
                            .unwrap_or(false);
                        let item_content = if is_inline {
                            vec![json!({ "type": "paragraph", "content": content })]
                        } else {
                            content
                        };
                        json!({ "type": "listItem", "content": item_content })
                    };
                    if let Some((_, parent)) = block_stack.last_mut() {
                        parent.push(node);
                    } else {
                        doc_content.push(node);
                    }
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
                    if let Some((_, parent)) = block_stack.last_mut() {
                        parent.push(node);
                    } else {
                        doc_content.push(node);
                    }
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
                    if let Some((_, parent)) = block_stack.last_mut() {
                        parent.push(node);
                    } else {
                        doc_content.push(node);
                    }
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
                    if let Some((_, parent)) = block_stack.last_mut() {
                        parent.push(node);
                    } else {
                        doc_content.push(node);
                    }
                }
            }

            // Body rows do emit explicit TableRow events.
            Event::Start(Tag::TableRow) => {
                block_stack.push(("tableRow".to_string(), Vec::new()));
            }
            Event::End(TagEnd::TableRow) => {
                if let Some((_, content)) = block_stack.pop() {
                    let node = json!({ "type": "tableRow", "content": content });
                    if let Some((_, parent)) = block_stack.last_mut() {
                        parent.push(node);
                    } else {
                        doc_content.push(node);
                    }
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
                    if let Some((_, parent)) = block_stack.last_mut() {
                        parent.push(node);
                    } else {
                        doc_content.push(node);
                    }
                }
            }

            // ── Leaf events ───────────────────────────────────────────────
            Event::Text(t) => {
                let marks: Vec<Value> = mark_stack
                    .iter()
                    .map(|m| {
                        if m == "link" {
                            json!({
                                "type": "link",
                                "attrs": { "href": link_href.as_deref().unwrap_or("") }
                            })
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
                let node = json!({
                    "type": "text",
                    "text": t.as_ref(),
                    "marks": [{ "type": "code" }]
                });
                inline_buf.push(node);
            }

            Event::SoftBreak => {
                // Soft breaks become a space to avoid word-joining
                inline_buf.push(json!({ "type": "text", "text": " " }));
            }

            Event::HardBreak => {
                inline_buf.push(json!({ "type": "hardBreak" }));
            }

            Event::Rule => {
                flush_inline(&mut inline_buf, &mut block_stack, &mut doc_content);
                doc_content.push(json!({ "type": "rule" }));
            }

            // Inline HTML tags (<b>, <br> etc.): pulldown-cmark surfaces the
            // text content between them via normal Text events, so the tag
            // tokens themselves can be silently ignored.
            Event::InlineHtml(_) => {}

            // Block HTML: the entire raw block (tags + content) arrives in one
            // event. Strip the tags and emit any remaining text so content is
            // not silently lost.
            Event::Html(raw) => {
                let stripped = strip_html_tags(raw.as_ref()).trim().to_string();
                if !stripped.is_empty() {
                    inline_buf.push(json!({ "type": "text", "text": stripped }));
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
pub fn adf_to_text(adf: &serde_json::Value) -> String {
    fn extract_text(value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::Object(obj) => {
                if let Some(text) = obj.get("text").and_then(|t| t.as_str()) {
                    return text.to_string();
                }
                if let Some(content) = obj.get("content") {
                    return extract_text(content);
                }
                String::new()
            }
            serde_json::Value::Array(arr) => {
                arr.iter().map(extract_text).collect::<Vec<_>>().join("")
            }
            _ => String::new(),
        }
    }
    extract_text(adf)
}

#[cfg(test)]
mod tests {
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
            .flat_map(|block| block["content"].as_array().unwrap_or(&vec![]).to_owned())
            .filter_map(|n| n["text"].as_str().map(|s| s.to_string()))
            .collect::<Vec<_>>()
            .join("");
        assert!(
            all_text.contains("block content"),
            "expected text from block HTML"
        );
    }
}
