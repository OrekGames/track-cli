use clap::{ArgGroup, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "track", version, about = "CLI for issue tracking systems")]
pub struct Cli {
    /// Output format
    #[arg(long, short = 'o', value_enum, global = true, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,

    /// Backend to use (youtrack, jira)
    #[arg(long, short = 'b', value_enum, global = true, default_value_t = Backend::YouTrack, env = "TRACKER_BACKEND")]
    pub backend: Backend,

    /// Path to a TOML config file
    #[arg(long, env = "TRACKER_CONFIG", global = true, value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// Tracker instance URL (overrides config file)
    #[arg(long, env = "TRACKER_URL", global = true)]
    pub url: Option<String>,

    /// API token (overrides config file)
    #[arg(long, env = "TRACKER_TOKEN", global = true)]
    pub token: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(ValueEnum, Clone, Debug, Copy, Default, PartialEq, Eq)]
pub enum Backend {
    /// YouTrack issue tracker
    #[default]
    YouTrack,
    // Future backends:
    // Jira,
    // Linear,
    // GitHub,
}

#[derive(ValueEnum, Clone, Debug, Copy)]
pub enum OutputFormat {
    Text,
    Json,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Issue operations
    #[command(visible_alias = "i")]
    Issue {
        #[command(subcommand)]
        action: IssueCommands,
    },
    /// Project operations
    #[command(visible_alias = "p")]
    Project {
        #[command(subcommand)]
        action: ProjectCommands,
    },
    /// Tag operations
    #[command(visible_alias = "t")]
    Tags {
        #[command(subcommand)]
        action: TagCommands,
    },
    /// Cache operations for offline context
    Cache {
        #[command(subcommand)]
        action: CacheCommands,
    },
    /// Local configuration (default project, etc.)
    #[command(visible_alias = "cfg")]
    Config {
        #[command(subcommand)]
        action: ConfigCommands,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigCommands {
    /// Set the default project for issue commands
    #[command(visible_alias = "proj")]
    Project {
        /// Project ID or shortName (e.g., "OGIT" or "0-2")
        id: String,
    },
    /// Show current local configuration
    Show,
    /// Clear local configuration (remove default project)
    Clear,
    /// Show local config file path
    Path,
}

#[derive(Subcommand, Debug)]
pub enum CacheCommands {
    /// Refresh the local cache with current tracker data
    Refresh,
    /// Show cached context (for AI assistants)
    Show,
    /// Show cache file path
    Path,
}

#[derive(Subcommand, Debug)]
pub enum IssueCommands {
    /// Get issue by ID
    #[command(visible_alias = "g")]
    Get {
        /// Issue ID (e.g., PROJ-123)
        id: String,
        /// Show full context (subtasks, links, comments)
        #[arg(long)]
        full: bool,
    },
    /// Create new issue
    #[command(visible_alias = "new", visible_alias = "c")]
    Create {
        /// Project ID or shortName (uses default project from 'track config project' if not specified)
        #[arg(long, short = 'p', conflicts_with = "json")]
        project: Option<String>,
        /// Issue summary
        #[arg(long, short = 's', required_unless_present = "json", conflicts_with = "json")]
        summary: Option<String>,
        /// Issue description
        #[arg(long, short = 'd', conflicts_with = "json")]
        description: Option<String>,
        /// Custom field value (format: FIELD=VALUE, can be repeated)
        #[arg(long = "field", short = 'f', value_name = "FIELD=VALUE", conflicts_with = "json")]
        fields: Vec<String>,
        /// Issue state (e.g., "Open", "In Progress")
        #[arg(long, conflicts_with = "json")]
        state: Option<String>,
        /// Issue priority (e.g., "Major", "Minor")
        #[arg(long, conflicts_with = "json")]
        priority: Option<String>,
        /// Assignee login
        #[arg(long, conflicts_with = "json")]
        assignee: Option<String>,
        /// Tag name (can be repeated)
        #[arg(long = "tag", short = 't', conflicts_with = "json")]
        tags: Vec<String>,
        /// Parent issue ID to create this as a subtask (e.g., PROJ-123)
        #[arg(long, conflicts_with = "json")]
        parent: Option<String>,
        /// JSON payload for issue creation
        #[arg(long, conflicts_with_all = ["project", "summary", "description", "fields", "state", "priority", "assignee", "tags", "parent"], value_name = "JSON")]
        json: Option<String>,
    },
    /// Update existing issue
    #[command(visible_alias = "u", group(
        ArgGroup::new("update_fields")
            .args(["summary", "description", "fields", "state", "priority", "assignee", "tags", "json"])
            .required(true)
            .multiple(true)
    ))]
    Update {
        /// Issue ID (e.g., PROJ-123)
        id: String,
        /// New summary
        #[arg(long, short = 's')]
        summary: Option<String>,
        /// New description
        #[arg(long, short = 'd')]
        description: Option<String>,
        /// Custom field value (format: FIELD=VALUE, can be repeated)
        #[arg(long = "field", short = 'f', value_name = "FIELD=VALUE", conflicts_with = "json")]
        fields: Vec<String>,
        /// Issue state (e.g., "Open", "In Progress")
        #[arg(long, conflicts_with = "json")]
        state: Option<String>,
        /// Issue priority (e.g., "Major", "Minor")
        #[arg(long, conflicts_with = "json")]
        priority: Option<String>,
        /// Assignee login
        #[arg(long, conflicts_with = "json")]
        assignee: Option<String>,
        /// Tag name (can be repeated)
        #[arg(long = "tag", short = 't', conflicts_with = "json")]
        tags: Vec<String>,
        /// JSON payload for issue update
        #[arg(long, conflicts_with_all = ["summary", "description", "fields", "state", "priority", "assignee", "tags"], value_name = "JSON")]
        json: Option<String>,
    },
    /// Search issues
    #[command(visible_alias = "s", visible_alias = "find")]
    Search {
        /// Search query (e.g., "project: MyProject #Unresolved")
        query: String,
        /// Maximum number of results
        #[arg(long, default_value_t = 20)]
        limit: usize,
        /// Number of results to skip
        #[arg(long, default_value_t = 0)]
        skip: usize,
    },
    /// Delete issue by ID
    #[command(visible_alias = "rm", visible_alias = "del")]
    Delete {
        /// Issue ID (e.g., PROJ-123)
        id: String,
    },
    /// Add a comment to an issue
    #[command(visible_alias = "cmt")]
    Comment {
        /// Issue ID (e.g., PROJ-123)
        id: String,
        /// Comment text
        #[arg(short = 'm', long = "message")]
        text: String,
    },
    /// List comments on an issue
    Comments {
        /// Issue ID (e.g., PROJ-123)
        id: String,
        /// Maximum number of comments to show
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Link two issues together
    Link {
        /// Source issue ID (e.g., PROJ-123)
        source: String,
        /// Target issue ID (e.g., PROJ-456)
        target: String,
        /// Link type: relates, depends, duplicates, subtask
        #[arg(long = "type", short = 't', default_value = "relates")]
        link_type: String,
    },
    /// Start work on an issue (set state to in-progress)
    Start {
        /// Issue ID (e.g., PROJ-123)
        id: String,
        /// State field name (default: "Stage")
        #[arg(long, default_value = "Stage")]
        field: String,
        /// State value for in-progress (default: "Develop")
        #[arg(long, default_value = "Develop")]
        state: String,
    },
    /// Complete an issue (set state to done/resolved)
    #[command(visible_alias = "done", visible_alias = "resolve")]
    Complete {
        /// Issue ID (e.g., PROJ-123)
        id: String,
        /// State field name (default: "Stage")
        #[arg(long, default_value = "Stage")]
        field: String,
        /// State value for done (default: "Done")
        #[arg(long, default_value = "Done")]
        state: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum ProjectCommands {
    /// List all projects
    #[command(visible_alias = "ls")]
    List,
    /// Get project by ID or shortName
    #[command(visible_alias = "g")]
    Get {
        /// Project ID or short name
        id: String,
    },
    /// List custom fields for a project
    #[command(visible_alias = "f")]
    Fields {
        /// Project ID or short name (e.g., "OGIT" or "0-2")
        id: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum TagCommands {
    /// List all available tags
    #[command(visible_alias = "ls")]
    List,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_global_flags_for_issue_get() {
        let cli = Cli::parse_from([
            "track",
            "--format",
            "json",
            "--url",
            "https://youtrack.example.com",
            "--token",
            "perm:token",
            "issue",
            "get",
            "PROJ-1",
        ]);

        assert!(matches!(cli.format, OutputFormat::Json));
        assert_eq!(cli.url.as_deref(), Some("https://youtrack.example.com"));
        assert_eq!(cli.token.as_deref(), Some("perm:token"));

        match cli.command {
            Commands::Issue { action } => match action {
                IssueCommands::Get { id, full } => {
                    assert_eq!(id, "PROJ-1");
                    assert!(!full); // Default is false
                }
                _ => panic!("expected issue get"),
            },
            _ => panic!("expected issue command"),
        }
    }

    #[test]
    fn parses_issue_create_parameters() {
        let cli = Cli::parse_from([
            "track",
            "issue",
            "create",
            "--project",
            "PROJ",
            "--summary",
            "Test summary",
        ]);

        match cli.command {
            Commands::Issue { action } => match action {
                IssueCommands::Create {
                    project,
                    summary,
                    description,
                    json,
                    ..
                } => {
                    assert_eq!(project.as_deref(), Some("PROJ"));
                    assert_eq!(summary.as_deref(), Some("Test summary"));
                    assert!(description.is_none());
                    assert!(json.is_none());
                }
                _ => panic!("expected issue create"),
            },
            _ => panic!("expected issue command"),
        }
    }

    #[test]
    fn parses_config_path_flag() {
        let cli = Cli::parse_from(["track", "--config", "/tmp/track-config.toml", "project", "list"]);

        assert_eq!(cli.config, Some(PathBuf::from("/tmp/track-config.toml")));
    }

    #[test]
    fn rejects_create_json_with_fields() {
        let result = Cli::try_parse_from([
            "track",
            "issue",
            "create",
            "--json",
            "{\"summary\":\"Test\",\"project\":\"PROJ\"}",
            "--summary",
            "Oops",
            "--project",
            "PROJ",
        ]);

        assert!(result.is_err());
    }

    #[test]
    fn rejects_update_without_fields() {
        let result = Cli::try_parse_from(["track", "issue", "update", "PROJ-1"]);

        assert!(result.is_err());
    }

    #[test]
    fn parses_update_json_payload() {
        let cli = Cli::parse_from([
            "track",
            "issue",
            "update",
            "PROJ-1",
            "--json",
            "{\"summary\":\"Updated\"}",
        ]);

        match cli.command {
            Commands::Issue { action } => match action {
                IssueCommands::Update { id, json, .. } => {
                    assert_eq!(id, "PROJ-1");
                    assert_eq!(json.as_deref(), Some("{\"summary\":\"Updated\"}"));
                }
                _ => panic!("expected issue update"),
            },
            _ => panic!("expected issue command"),
        }
    }

    #[test]
    fn parses_create_with_custom_fields_and_tags() {
        let cli = Cli::parse_from([
            "track",
            "issue",
            "create",
            "-p",
            "PROJ",
            "-s",
            "Bug fix",
            "--state",
            "Open",
            "--priority",
            "Major",
            "--assignee",
            "john.doe",
            "--tag",
            "bug",
            "--tag",
            "urgent",
            "--field",
            "Type=Bug",
        ]);

        match cli.command {
            Commands::Issue { action } => match action {
                IssueCommands::Create {
                    project,
                    summary,
                    state,
                    priority,
                    assignee,
                    tags,
                    fields,
                    ..
                } => {
                    assert_eq!(project.as_deref(), Some("PROJ"));
                    assert_eq!(summary.as_deref(), Some("Bug fix"));
                    assert_eq!(state.as_deref(), Some("Open"));
                    assert_eq!(priority.as_deref(), Some("Major"));
                    assert_eq!(assignee.as_deref(), Some("john.doe"));
                    assert_eq!(tags, vec!["bug", "urgent"]);
                    assert_eq!(fields, vec!["Type=Bug"]);
                }
                _ => panic!("expected issue create"),
            },
            _ => panic!("expected issue command"),
        }
    }

    #[test]
    fn parses_update_with_state_only() {
        let cli = Cli::parse_from([
            "track",
            "issue",
            "update",
            "PROJ-1",
            "--state",
            "Resolved",
        ]);

        match cli.command {
            Commands::Issue { action } => match action {
                IssueCommands::Update { id, state, .. } => {
                    assert_eq!(id, "PROJ-1");
                    assert_eq!(state.as_deref(), Some("Resolved"));
                }
                _ => panic!("expected issue update"),
            },
            _ => panic!("expected issue command"),
        }
    }

    #[test]
    fn parses_create_with_parent_flag() {
        let cli = Cli::parse_from([
            "track",
            "issue",
            "create",
            "-p",
            "PROJ",
            "-s",
            "Subtask summary",
            "--parent",
            "PROJ-100",
        ]);

        match cli.command {
            Commands::Issue { action } => match action {
                IssueCommands::Create {
                    project,
                    summary,
                    parent,
                    ..
                } => {
                    assert_eq!(project.as_deref(), Some("PROJ"));
                    assert_eq!(summary.as_deref(), Some("Subtask summary"));
                    assert_eq!(parent.as_deref(), Some("PROJ-100"));
                }
                _ => panic!("expected issue create"),
            },
            _ => panic!("expected issue command"),
        }
    }

    #[test]
    fn parses_create_without_parent_flag() {
        let cli = Cli::parse_from([
            "track",
            "issue",
            "create",
            "-p",
            "PROJ",
            "-s",
            "Regular issue",
        ]);

        match cli.command {
            Commands::Issue { action } => match action {
                IssueCommands::Create { parent, .. } => {
                    assert!(parent.is_none());
                }
                _ => panic!("expected issue create"),
            },
            _ => panic!("expected issue command"),
        }
    }

    #[test]
    fn parses_comment_command() {
        let cli = Cli::parse_from([
            "track",
            "issue",
            "comment",
            "PROJ-123",
            "-m",
            "This is a comment",
        ]);

        match cli.command {
            Commands::Issue { action } => match action {
                IssueCommands::Comment { id, text } => {
                    assert_eq!(id, "PROJ-123");
                    assert_eq!(text, "This is a comment");
                }
                _ => panic!("expected issue comment"),
            },
            _ => panic!("expected issue command"),
        }
    }

    #[test]
    fn parses_comments_command() {
        let cli = Cli::parse_from(["track", "issue", "comments", "PROJ-123", "--limit", "5"]);

        match cli.command {
            Commands::Issue { action } => match action {
                IssueCommands::Comments { id, limit } => {
                    assert_eq!(id, "PROJ-123");
                    assert_eq!(limit, 5);
                }
                _ => panic!("expected issue comments"),
            },
            _ => panic!("expected issue command"),
        }
    }

    #[test]
    fn parses_link_command_with_default_type() {
        let cli = Cli::parse_from(["track", "issue", "link", "PROJ-123", "PROJ-456"]);

        match cli.command {
            Commands::Issue { action } => match action {
                IssueCommands::Link {
                    source,
                    target,
                    link_type,
                } => {
                    assert_eq!(source, "PROJ-123");
                    assert_eq!(target, "PROJ-456");
                    assert_eq!(link_type, "relates");
                }
                _ => panic!("expected issue link"),
            },
            _ => panic!("expected issue command"),
        }
    }

    #[test]
    fn parses_link_command_with_custom_type() {
        let cli = Cli::parse_from([
            "track", "issue", "link", "PROJ-123", "PROJ-456", "-t", "depends",
        ]);

        match cli.command {
            Commands::Issue { action } => match action {
                IssueCommands::Link {
                    source,
                    target,
                    link_type,
                } => {
                    assert_eq!(source, "PROJ-123");
                    assert_eq!(target, "PROJ-456");
                    assert_eq!(link_type, "depends");
                }
                _ => panic!("expected issue link"),
            },
            _ => panic!("expected issue command"),
        }
    }

    #[test]
    fn parses_start_command_with_defaults() {
        let cli = Cli::parse_from(["track", "issue", "start", "PROJ-123"]);

        match cli.command {
            Commands::Issue { action } => match action {
                IssueCommands::Start { id, field, state } => {
                    assert_eq!(id, "PROJ-123");
                    assert_eq!(field, "Stage");
                    assert_eq!(state, "Develop");
                }
                _ => panic!("expected issue start"),
            },
            _ => panic!("expected issue command"),
        }
    }

    #[test]
    fn parses_complete_command_with_custom_values() {
        let cli = Cli::parse_from([
            "track",
            "issue",
            "complete",
            "PROJ-123",
            "--field",
            "State",
            "--state",
            "Resolved",
        ]);

        match cli.command {
            Commands::Issue { action } => match action {
                IssueCommands::Complete { id, field, state } => {
                    assert_eq!(id, "PROJ-123");
                    assert_eq!(field, "State");
                    assert_eq!(state, "Resolved");
                }
                _ => panic!("expected issue complete"),
            },
            _ => panic!("expected issue command"),
        }
    }

    #[test]
    fn parses_done_alias_for_complete() {
        let cli = Cli::parse_from(["track", "issue", "done", "PROJ-123"]);

        match cli.command {
            Commands::Issue { action } => match action {
                IssueCommands::Complete { id, .. } => {
                    assert_eq!(id, "PROJ-123");
                }
                _ => panic!("expected issue complete via done alias"),
            },
            _ => panic!("expected issue command"),
        }
    }

    #[test]
    fn parses_get_with_full_flag() {
        let cli = Cli::parse_from(["track", "issue", "get", "PROJ-123", "--full"]);

        match cli.command {
            Commands::Issue { action } => match action {
                IssueCommands::Get { id, full } => {
                    assert_eq!(id, "PROJ-123");
                    assert!(full);
                }
                _ => panic!("expected issue get"),
            },
            _ => panic!("expected issue command"),
        }
    }
}
