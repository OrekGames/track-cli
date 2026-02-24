use crate::cli::OutputFormat;
use colored::Colorize;
use serde::Serialize;
use tracker_core::{
    Article, ArticleAttachment, BundleDefinition, Comment, CustomField, CustomFieldDefinition,
    Issue, IssueTag, Project, ProjectCustomField,
};

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
            serde_json::to_string_pretty(&json_err)
                .unwrap_or_else(|_| format!(r#"{{"error": true, "message": "{}"}}"#, err))
        }
        OutputFormat::Text => format!("{}: {:#}", "Error".red().bold(), err),
    };
    eprintln!("{}", message);
}

/// Output a progress message to stderr if stdout is a TTY and format is text
pub fn output_progress(message: &str, format: OutputFormat) {
    if format == OutputFormat::Text && atty::is(atty::Stream::Stdout) {
        use colored::Colorize;
        eprintln!("{} {}", "→".cyan().bold(), message);
    }
}

/// Print a pagination hint to stderr when results fill the page limit.
/// Only prints in text mode; no TTY gate (goes to stderr, won't pollute piped output).
pub fn output_page_hint(
    result_count: usize,
    limit: usize,
    skip: usize,
    cached_total: Option<(u64, &str)>,
    format: OutputFormat,
) {
    if format != OutputFormat::Text {
        return;
    }
    if result_count == 0 || result_count < limit {
        return; // partial page or empty — we have all results
    }
    // Full page — there may be more
    let next_skip = skip + result_count;

    let total_part = match cached_total {
        Some((total, "live")) => format!(" ({} of {} total)", result_count, total),
        Some((total, age)) => format!(" (~{} total, {})", total, age),
        None => String::new(),
    };

    if skip == 0 {
        // First page: suggest both --all and --skip
        eprintln!(
            "  {} {} results shown{}  ·  use {} or {} for next page",
            "┄┄".dimmed(),
            result_count,
            total_part.dimmed(),
            "--all".cyan(),
            format!("--skip {}", next_skip).cyan(),
        );
    } else {
        // Already paginating with --skip: only suggest --skip (--all conflicts with --skip in clap)
        eprintln!(
            "  {} {} results shown{}  ·  use {} for next page",
            "┄┄".dimmed(),
            result_count,
            total_part.dimmed(),
            format!("--skip {}", next_skip).cyan(),
        );
    }
}

pub trait Displayable {
    fn display(&self) -> String;
}

impl Displayable for Issue {
    fn display(&self) -> String {
        let mut output = format!(
            "{} - {}\n  {}: {}\n  {}: {}\n  {}: {}",
            self.id_readable.cyan().bold(),
            self.summary.white().bold(),
            "Project".dimmed(),
            self.project
                .short_name
                .as_deref()
                .unwrap_or(&self.project.id),
            "Created".dimmed(),
            self.created
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
                .dimmed(),
            "Updated".dimmed(),
            self.updated
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
                .dimmed()
        );

        if let Some(desc) = &self.description {
            output.push_str(&format!("\n  {}: {}", "Description".dimmed(), desc));
        }

        if !self.tags.is_empty() {
            let tag_names: Vec<String> = self
                .tags
                .iter()
                .map(|t| t.name.magenta().to_string())
                .collect();
            output.push_str(&format!(
                "\n  {}: {}",
                "Tags".dimmed(),
                tag_names.join(", ")
            ));
        }

        if !self.custom_fields.is_empty() {
            output.push_str(&format!("\n  {}:", "Custom Fields".dimmed()));
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
                let val = value.as_deref().unwrap_or("None");
                let colored_val = colorize_priority(name, val);
                format!("{}: {}", name.dimmed(), colored_val)
            }
            CustomField::State {
                name,
                value,
                is_resolved,
            } => {
                let val = value.as_deref().unwrap_or("None");
                let colored_val = if *is_resolved {
                    val.green().to_string()
                } else if val.to_lowercase().contains("progress") {
                    val.yellow().to_string()
                } else {
                    val.to_string()
                };
                format!("{}: {}", name.dimmed(), colored_val)
            }
            CustomField::SingleUser { name, login, .. } => {
                format!("{}: {}", name.dimmed(), login.as_deref().unwrap_or("None"))
            }
            CustomField::Text { name, value } => {
                format!("{}: {}", name.dimmed(), value.as_deref().unwrap_or("None"))
            }
            CustomField::Unknown { name } => {
                format!("{}: {}", name.dimmed(), "Unknown field".dimmed())
            }
        }
    }
}

fn colorize_priority(field_name: &str, value: &str) -> String {
    if field_name.to_lowercase() == "priority" {
        match value.to_lowercase().as_str() {
            "critical" | "show-stopper" => value.red().bold().to_string(),
            "major" | "high" => value.red().to_string(),
            "minor" | "low" => value.dimmed().to_string(),
            _ => value.to_string(),
        }
    } else {
        value.to_string()
    }
}

impl Displayable for Project {
    fn display(&self) -> String {
        let mut output = format!(
            "{} ({}) - {}",
            self.short_name.cyan().bold(),
            self.id.dimmed(),
            self.name.white().bold()
        );
        if let Some(desc) = &self.description {
            output.push_str(&format!("\n  {}: {}", "Description".dimmed(), desc));
        }
        output
    }
}

impl Displayable for ProjectCustomField {
    fn display(&self) -> String {
        let required = if self.required {
            " (required)".yellow().to_string()
        } else {
            String::new()
        };
        format!(
            "{} [{}]{}",
            self.name.white().bold(),
            self.field_type.dimmed(),
            required
        )
    }
}

impl Displayable for IssueTag {
    fn display(&self) -> String {
        format!("{} ({})", self.name.magenta(), self.id.dimmed())
    }
}

impl Displayable for Article {
    fn display(&self) -> String {
        let mut output = format!(
            "{} - {}\n  {}: {}\n  {}: {}\n  {}: {}",
            self.id_readable.cyan().bold(),
            self.summary.white().bold(),
            "Project".dimmed(),
            self.project
                .short_name
                .as_deref()
                .unwrap_or(&self.project.id),
            "Created".dimmed(),
            self.created
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
                .dimmed(),
            "Updated".dimmed(),
            self.updated
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
                .dimmed()
        );

        if let Some(parent) = &self.parent_article {
            output.push_str(&format!(
                "\n  {}: {}",
                "Parent".dimmed(),
                parent.id_readable.as_deref().unwrap_or(&parent.id).cyan()
            ));
        }

        if self.has_children {
            output.push_str(&format!(
                "\n  {}: {}",
                "Has children".dimmed(),
                "yes".green()
            ));
        }

        if !self.tags.is_empty() {
            let tag_names: Vec<String> = self
                .tags
                .iter()
                .map(|t| t.name.magenta().to_string())
                .collect();
            output.push_str(&format!(
                "\n  {}: {}",
                "Tags".dimmed(),
                tag_names.join(", ")
            ));
        }

        if let Some(content) = &self.content {
            // Truncate content for display
            let preview: String = content.chars().take(200).collect();
            if content.len() > 200 {
                output.push_str(&format!("\n  {}: {}...", "Content".dimmed(), preview));
            } else {
                output.push_str(&format!("\n  {}: {}", "Content".dimmed(), preview));
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
            self.name.white().bold(),
            self.mime_type.as_deref().unwrap_or("unknown").dimmed(),
            size_str.dimmed()
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

        format!("[{}] {} - {}", date.dimmed(), author.cyan(), self.text)
    }
}

impl Displayable for CustomFieldDefinition {
    fn display(&self) -> String {
        format!(
            "{} [{}] ({})",
            self.name.white().bold(),
            self.field_type.dimmed(),
            self.id.dimmed()
        )
    }
}

impl Displayable for BundleDefinition {
    fn display(&self) -> String {
        let values: Vec<&str> = self.values.iter().map(|v| v.name.as_str()).collect();
        let values_str = if values.is_empty() {
            "(no values)".dimmed().to_string()
        } else {
            values.join(", ")
        };

        format!(
            "{} [{}] ({})\n  {}: {}",
            self.name.white().bold(),
            self.bundle_type.dimmed(),
            self.id.dimmed(),
            "Values".dimmed(),
            values_str
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_hint_suppressed_on_partial_page() {
        // limit=20, got 7 — no hint expected (just verify it returns without panic)
        output_page_hint(7, 20, 0, None, OutputFormat::Text);
    }

    #[test]
    fn page_hint_suppressed_in_json_mode() {
        output_page_hint(20, 20, 0, None, OutputFormat::Json);
    }

    #[test]
    fn page_hint_suppressed_on_zero_results() {
        // Edge case: 0 results with limit=0 should not fire
        output_page_hint(0, 0, 0, None, OutputFormat::Text);
    }

    #[test]
    fn page_hint_fires_on_full_page() {
        // limit=20, got 20 — hint should fire (we can't capture stderr easily in unit test,
        // so just verify it doesn't panic)
        output_page_hint(20, 20, 0, None, OutputFormat::Text);
    }

    #[test]
    fn page_hint_with_skip() {
        // When skip > 0, should still work but only suggest --skip
        output_page_hint(20, 20, 40, None, OutputFormat::Text);
    }

    #[test]
    fn page_hint_with_cached_total() {
        // Verify it doesn't panic with a cached total
        output_page_hint(20, 20, 0, Some((847, "2h ago")), OutputFormat::Text);
    }
}
