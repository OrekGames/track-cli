use crate::cli::{ArticleCommands, OutputFormat};
use crate::output::{output_list, output_result};
use anyhow::{Context, Result};
use std::fs;
use tracker_core::{CreateArticle, IssueTracker, KnowledgeBase, UpdateArticle};

pub fn handle_article(
    issue_client: &dyn IssueTracker,
    kb_client: &dyn KnowledgeBase,
    action: &ArticleCommands,
    format: OutputFormat,
) -> Result<()> {
    match action {
        ArticleCommands::Get { id } => handle_get(kb_client, id, format),
        ArticleCommands::List {
            project,
            limit,
            skip,
        } => handle_list(kb_client, project.as_deref(), *limit, *skip, format),
        ArticleCommands::Search { query, limit, skip } => {
            handle_search(kb_client, query, *limit, *skip, format)
        }
        ArticleCommands::Create {
            project,
            summary,
            content,
            content_file,
            parent,
            tags,
        } => handle_create(
            issue_client,
            kb_client,
            project,
            summary,
            content.as_deref(),
            content_file.as_deref(),
            parent.as_deref(),
            tags,
            format,
        ),
        ArticleCommands::Update {
            id,
            summary,
            content,
            content_file,
            tags,
        } => handle_update(
            kb_client,
            id,
            summary.as_deref(),
            content.as_deref(),
            content_file.as_deref(),
            tags,
            format,
        ),
        ArticleCommands::Delete { id } => handle_delete(kb_client, id),
        ArticleCommands::Tree { id } => handle_tree(kb_client, id, format),
        ArticleCommands::Move { id, parent } => {
            handle_move(kb_client, id, parent.as_deref(), format)
        }
        ArticleCommands::Attachments { id } => handle_attachments(kb_client, id, format),
        ArticleCommands::Comment { id, text } => handle_comment(kb_client, id, text, format),
        ArticleCommands::Comments { id, limit } => handle_comments(kb_client, id, *limit, format),
    }
}

fn handle_get(client: &dyn KnowledgeBase, id: &str, format: OutputFormat) -> Result<()> {
    let article = client
        .get_article(id)
        .with_context(|| format!("Failed to fetch article '{}'", id))?;

    output_result(&article, format);
    Ok(())
}

fn handle_list(
    client: &dyn KnowledgeBase,
    project: Option<&str>,
    limit: usize,
    skip: usize,
    format: OutputFormat,
) -> Result<()> {
    let articles = client
        .list_articles(project, limit, skip)
        .context("Failed to list articles")?;

    output_list(&articles, format);
    Ok(())
}

fn handle_search(
    client: &dyn KnowledgeBase,
    query: &str,
    limit: usize,
    skip: usize,
    format: OutputFormat,
) -> Result<()> {
    let articles = client
        .search_articles(query, limit, skip)
        .with_context(|| format!("Failed to search articles with query '{}'", query))?;

    output_list(&articles, format);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn handle_create(
    issue_client: &dyn IssueTracker,
    kb_client: &dyn KnowledgeBase,
    project: &str,
    summary: &str,
    content: Option<&str>,
    content_file: Option<&std::path::Path>,
    parent: Option<&str>,
    tags: &[String],
    format: OutputFormat,
) -> Result<()> {
    // Resolve project shortName to internal ID
    let project_id = issue_client
        .resolve_project_id(project)
        .with_context(|| format!("Failed to resolve project '{}'", project))?;

    // Resolve parent article ID if provided (fetch article to get internal ID)
    let parent_article_id = if let Some(parent_id) = parent {
        let parent_article = kb_client
            .get_article(parent_id)
            .with_context(|| format!("Failed to resolve parent article '{}'", parent_id))?;
        Some(parent_article.id)
    } else {
        None
    };

    // Read content from file if specified
    let content = if let Some(file_path) = content_file {
        Some(
            fs::read_to_string(file_path)
                .with_context(|| format!("Failed to read content from '{}'", file_path.display()))?,
        )
    } else {
        content.map(String::from)
    };

    let create = CreateArticle {
        project_id,
        summary: summary.to_string(),
        content,
        parent_article_id,
        tags: tags.to_vec(),
    };

    let article = kb_client
        .create_article(&create)
        .context("Failed to create article")?;

    output_result(&article, format);
    Ok(())
}

fn handle_update(
    client: &dyn KnowledgeBase,
    id: &str,
    summary: Option<&str>,
    content: Option<&str>,
    content_file: Option<&std::path::Path>,
    tags: &[String],
    format: OutputFormat,
) -> Result<()> {
    // Read content from file if specified
    let content = if let Some(file_path) = content_file {
        Some(
            fs::read_to_string(file_path)
                .with_context(|| format!("Failed to read content from '{}'", file_path.display()))?,
        )
    } else {
        content.map(String::from)
    };

    let update = UpdateArticle {
        summary: summary.map(String::from),
        content,
        tags: tags.to_vec(),
    };

    let article = client
        .update_article(id, &update)
        .with_context(|| format!("Failed to update article '{}'", id))?;

    output_result(&article, format);
    Ok(())
}

fn handle_delete(client: &dyn KnowledgeBase, id: &str) -> Result<()> {
    client
        .delete_article(id)
        .with_context(|| format!("Failed to delete article '{}'", id))?;

    println!("Article '{}' deleted.", id);
    Ok(())
}

fn handle_tree(client: &dyn KnowledgeBase, id: &str, format: OutputFormat) -> Result<()> {
    // First get the parent article
    let parent = client
        .get_article(id)
        .with_context(|| format!("Failed to fetch article '{}'", id))?;

    // Then get children
    let children = client
        .get_child_articles(id)
        .with_context(|| format!("Failed to fetch child articles for '{}'", id))?;

    match format {
        OutputFormat::Json => {
            #[derive(serde::Serialize)]
            struct ArticleTree {
                article: tracker_core::Article,
                children: Vec<tracker_core::Article>,
            }
            let tree = ArticleTree {
                article: parent,
                children,
            };
            println!("{}", serde_json::to_string_pretty(&tree).unwrap());
        }
        OutputFormat::Text => {
            println!("{} - {}", parent.id_readable, parent.summary);
            for child in &children {
                println!("  {} - {}", child.id_readable, child.summary);
            }
            if children.is_empty() {
                println!("  (no children)");
            }
        }
    }

    Ok(())
}

fn handle_move(
    client: &dyn KnowledgeBase,
    id: &str,
    new_parent: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    let article = client
        .move_article(id, new_parent)
        .with_context(|| format!("Failed to move article '{}'", id))?;

    match new_parent {
        Some(parent) => println!("Article '{}' moved to parent '{}'.", id, parent),
        None => println!("Article '{}' moved to root.", id),
    }

    output_result(&article, format);
    Ok(())
}

fn handle_attachments(
    client: &dyn KnowledgeBase,
    id: &str,
    format: OutputFormat,
) -> Result<()> {
    let attachments = client
        .list_article_attachments(id)
        .with_context(|| format!("Failed to list attachments for article '{}'", id))?;

    output_list(&attachments, format);
    Ok(())
}

fn handle_comment(
    client: &dyn KnowledgeBase,
    id: &str,
    text: &str,
    format: OutputFormat,
) -> Result<()> {
    let comment = client
        .add_article_comment(id, text)
        .with_context(|| format!("Failed to add comment to article '{}'", id))?;

    output_result(&comment, format);
    Ok(())
}

fn handle_comments(
    client: &dyn KnowledgeBase,
    id: &str,
    limit: usize,
    format: OutputFormat,
) -> Result<()> {
    let comments = client
        .get_article_comments(id)
        .with_context(|| format!("Failed to fetch comments for article '{}'", id))?;

    // Apply limit
    let comments: Vec<_> = comments.into_iter().take(limit).collect();

    output_list(&comments, format);
    Ok(())
}
