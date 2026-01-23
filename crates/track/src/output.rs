use crate::cli::OutputFormat;
use serde::Serialize;
use tracker_core::{Article, ArticleAttachment, Comment, CustomField, Issue, IssueTag, Project, ProjectCustomField};

pub fn output_result<T: Serialize + Displayable>(result: &T, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            if let Ok(json) = serde_json::to_string_pretty(result) {
                println!("{}", json);
            }
        }
        OutputFormat::Text => {
            println!("{}", result.display());
        }
    }
}

pub fn output_list<T: Serialize + Displayable>(items: &[T], format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            if let Ok(json) = serde_json::to_string_pretty(&items) {
                println!("{}", json);
            }
        }
        OutputFormat::Text => {
            for item in items {
                println!("{}", item.display());
                println!();
            }
        }
    }
}

#[derive(Serialize)]
pub struct JsonError {
    pub error: bool,
    pub code: String,
    pub message: String,
}

pub fn output_error(err: &anyhow::Error, format: OutputFormat) {
    let message = match format {
        OutputFormat::Json => {
            let json_err = JsonError {
                error: true,
                code: "error".to_string(),
                message: format!("{:#}", err),
            };
            serde_json::to_string_pretty(&json_err).unwrap_or_else(|_| {
                format!(r#"{{"error": true, "message": "{}"}}"#, err)
            })
        }
        OutputFormat::Text => format!("Error: {:#}", err),
    };
    eprintln!("{}", message);
}

pub trait Displayable {
    fn display(&self) -> String;
}

impl Displayable for Issue {
    fn display(&self) -> String {
        let mut output = format!(
            "{} - {}\n  Project: {}\n  Created: {}\n  Updated: {}",
            self.id_readable,
            self.summary,
            self.project.short_name.as_deref().unwrap_or(&self.project.id),
            self.created.format("%Y-%m-%d %H:%M:%S"),
            self.updated.format("%Y-%m-%d %H:%M:%S")
        );

        if let Some(desc) = &self.description {
            output.push_str(&format!("\n  Description: {}", desc));
        }

        if !self.tags.is_empty() {
            let tag_names: Vec<&str> = self.tags.iter().map(|t| t.name.as_str()).collect();
            output.push_str(&format!("\n  Tags: {}", tag_names.join(", ")));
        }

        if !self.custom_fields.is_empty() {
            output.push_str("\n  Custom Fields:");
            for field in &self.custom_fields {
                output.push_str(&format!("\n    {}", field.display()));
            }
        }

        output
    }
}

impl Displayable for CustomField {
    fn display(&self) -> String {
        match self {
            CustomField::SingleEnum { name, value } => {
                format!("{}: {}", name, value.as_deref().unwrap_or("None"))
            }
            CustomField::State { name, value, .. } => {
                format!("{}: {}", name, value.as_deref().unwrap_or("None"))
            }
            CustomField::SingleUser { name, login, .. } => {
                format!("{}: {}", name, login.as_deref().unwrap_or("None"))
            }
            CustomField::Text { name, value } => {
                format!("{}: {}", name, value.as_deref().unwrap_or("None"))
            }
            CustomField::Unknown { name } => format!("{}: Unknown field", name),
        }
    }
}

impl Displayable for Project {
    fn display(&self) -> String {
        let mut output = format!("{} ({}) - {}", self.short_name, self.id, self.name);
        if let Some(desc) = &self.description {
            output.push_str(&format!("\n  Description: {}", desc));
        }
        output
    }
}

impl Displayable for ProjectCustomField {
    fn display(&self) -> String {
        let required = if self.required { " (required)" } else { "" };
        format!("{} [{}]{}", self.name, self.field_type, required)
    }
}

impl Displayable for IssueTag {
    fn display(&self) -> String {
        format!("{} ({})", self.name, self.id)
    }
}

impl Displayable for Article {
    fn display(&self) -> String {
        let mut output = format!(
            "{} - {}\n  Project: {}\n  Created: {}\n  Updated: {}",
            self.id_readable,
            self.summary,
            self.project.short_name.as_deref().unwrap_or(&self.project.id),
            self.created.format("%Y-%m-%d %H:%M:%S"),
            self.updated.format("%Y-%m-%d %H:%M:%S")
        );

        if let Some(parent) = &self.parent_article {
            output.push_str(&format!(
                "\n  Parent: {}",
                parent.id_readable.as_deref().unwrap_or(&parent.id)
            ));
        }

        if self.has_children {
            output.push_str("\n  Has children: yes");
        }

        if !self.tags.is_empty() {
            let tag_names: Vec<&str> = self.tags.iter().map(|t| t.name.as_str()).collect();
            output.push_str(&format!("\n  Tags: {}", tag_names.join(", ")));
        }

        if let Some(content) = &self.content {
            // Truncate content for display
            let preview: String = content.chars().take(200).collect();
            if content.len() > 200 {
                output.push_str(&format!("\n  Content: {}...", preview));
            } else {
                output.push_str(&format!("\n  Content: {}", preview));
            }
        }

        output
    }
}

impl Displayable for ArticleAttachment {
    fn display(&self) -> String {
        let size_str = if self.size > 1024 * 1024 {
            format!("{:.1} MB", self.size as f64 / (1024.0 * 1024.0))
        } else if self.size > 1024 {
            format!("{:.1} KB", self.size as f64 / 1024.0)
        } else {
            format!("{} bytes", self.size)
        };

        format!(
            "{} ({}) - {}",
            self.name,
            self.mime_type.as_deref().unwrap_or("unknown"),
            size_str
        )
    }
}

impl Displayable for Comment {
    fn display(&self) -> String {
        let author = self
            .author
            .as_ref()
            .map(|a| a.name.as_deref().unwrap_or(&a.login))
            .unwrap_or("Unknown");

        let date = self
            .created
            .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "Unknown date".to_string());

        format!("[{}] {} - {}", date, author, self.text)
    }
}
