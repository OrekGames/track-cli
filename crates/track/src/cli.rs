use clap::{ArgGroup, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "track", version, about = "CLI for issue tracking systems")]
pub struct Cli {
    /// Output format
    #[arg(long, short = 'o', value_enum, global = true, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,

    /// Verbose output (shows detailed changes and additional context)
    #[arg(long, short = 'v', global = true)]
    pub verbose: bool,

    /// When to colorize output
    #[arg(long, value_enum, global = true, default_value_t = ColorChoice::Auto)]
    pub color: ColorChoice,

    /// Backend to use (youtrack, jira, github, gitlab). If not specified, uses config or defaults to YouTrack.
    #[arg(long, short = 'b', value_enum, global = true, env = "TRACKER_BACKEND")]
    pub backend: Option<Backend>,

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

#[derive(ValueEnum, Clone, Debug, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    /// YouTrack issue tracker
    #[default]
    #[value(name = "youtrack", alias = "yt")]
    YouTrack,
    /// Jira issue tracker
    #[value(name = "jira", alias = "j")]
    Jira,
    /// GitHub issue tracker
    #[value(name = "github", alias = "gh")]
    GitHub,
    /// GitLab issue tracker
    #[value(name = "gitlab", alias = "gl")]
    GitLab,
}

impl std::fmt::Display for Backend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Backend::YouTrack => write!(f, "youtrack"),
            Backend::Jira => write!(f, "jira"),
            Backend::GitHub => write!(f, "github"),
            Backend::GitLab => write!(f, "gitlab"),
        }
    }
}

#[derive(ValueEnum, Clone, Debug, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    Json,
}

#[derive(ValueEnum, Clone, Debug, Copy, Default)]
pub enum ColorChoice {
    /// Colorize output if stdout is a terminal
    #[default]
    Auto,
    /// Always colorize output
    Always,
    /// Never colorize output
    Never,
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
    /// Evaluate AI agent performance against mock scenarios
    Eval {
        #[command(subcommand)]
        action: EvalCommands,
    },
    /// Local configuration (default project, etc.)
    #[command(visible_alias = "cfg")]
    Config {
        #[command(subcommand)]
        action: ConfigCommands,
    },
    /// Knowledge base article operations
    #[command(visible_alias = "wiki", visible_alias = "a")]
    Article {
        #[command(subcommand)]
        action: ArticleCommands,
    },
    /// Custom field admin operations (YouTrack only)
    Field {
        #[command(subcommand)]
        action: FieldCommands,
    },
    /// Bundle admin operations (YouTrack only)
    Bundle {
        #[command(subcommand)]
        action: BundleCommands,
    },
    /// Aggregate context for AI assistants (projects, fields, users, queries)
    #[command(visible_alias = "ctx")]
    Context {
        /// Specific project to focus context on
        #[arg(long, short = 'p')]
        project: Option<String>,

        /// Force refresh from API (ignore cached data)
        #[arg(long)]
        refresh: bool,

        /// Include unresolved issues in context
        #[arg(long)]
        include_issues: bool,

        /// Maximum issues to include when using --include-issues (default: 10)
        #[arg(long, default_value_t = 10)]
        issue_limit: usize,
    },
    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
    /// Initialize a .track.toml config file
    ///
    /// Creates a configuration file in the current directory (or globally with --global).
    /// Use 'track config keys' to see all available configuration keys.
    /// You can later modify the file with 'track config set'.
    ///
    /// Use --skills to install agent skill files globally for Claude, Copilot, Cursor, and Gemini.
    Init {
        /// Tracker API URL (e.g., https://youtrack.example.com, https://company.atlassian.net, https://api.github.com, or https://gitlab.com/api/v4)
        #[arg(long, required_unless_present = "skills")]
        url: Option<String>,
        /// API token for the selected backend
        #[arg(long, required_unless_present = "skills")]
        token: Option<String>,
        /// Project to validate. Required for GitHub as owner/repo and for GitLab as project ID/path.
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Backend to use. Defaults to youtrack.
        #[arg(long, short = 'b', value_enum, default_value_t = Backend::YouTrack)]
        backend: Backend,
        /// Email for Jira authentication (required for Jira backend, ignored for YouTrack)
        #[arg(long, short = 'e')]
        email: Option<String>,
        /// Install agent skill files globally for Claude, Copilot, Cursor, and Gemini
        #[arg(long)]
        skills: bool,
        /// Create config at global level (~/.tracker-cli/.track.toml) instead of local
        #[arg(long, short = 'g')]
        global: bool,
    },
    /// Open an issue or the tracker dashboard in your browser
    Open {
        /// Issue ID to open (e.g., PROJ-123). If omitted, opens the dashboard.
        id: Option<String>,
    },
    /// Shortcut: Get issue by ID (same as 'track issue get')
    #[command(external_subcommand)]
    External(Vec<String>),
}

impl Cli {
    /// Generate shell completions and write to stdout
    pub fn generate_completions(shell: Shell) {
        let mut cmd = Cli::command();
        clap_complete::generate(shell, &mut cmd, "track", &mut std::io::stdout());
    }
}

#[derive(Subcommand, Debug)]
pub enum ConfigCommands {
    /// Set a configuration value (use 'config keys' to see available keys)
    Set {
        /// Configuration key to set (e.g., "url", "token", "backend", "jira.email")
        key: String,
        /// Value to set
        value: String,
        /// Write to global config (~/.tracker-cli/.track.toml) instead of project config
        #[arg(long, short = 'g')]
        global: bool,
    },
    /// Get a configuration value (shows effective value with source)
    Get {
        /// Configuration key to get
        key: String,
    },
    /// Set the default project for issue commands
    #[command(visible_alias = "proj")]
    Project {
        /// Project ID or shortName (e.g., "OGIT" or "0-2")
        id: String,
    },
    /// Set the default backend (youtrack or jira)
    Backend {
        /// Backend name: youtrack (or yt), jira (or j)
        #[arg(value_enum)]
        backend: Backend,
    },
    /// Show configuration from all sources with origin markers
    Show,
    /// List all available configuration keys
    Keys,
    /// Clear configuration (remove default project and backend)
    Clear {
        /// Clear global config instead of project config
        #[arg(long, short = 'g')]
        global: bool,
    },
    /// Show config file paths
    Path,
    /// Test connection to the tracker (validates URL and token)
    Test,
}

#[derive(Subcommand, Debug)]
pub enum CacheCommands {
    /// Refresh the local cache with current tracker data
    Refresh {
        /// Only refresh if cache is older than specified duration (e.g., "1h", "30m", "1d")
        #[arg(long, value_name = "DURATION")]
        if_stale: Option<String>,
    },
    /// Show cache freshness status (age, last update time)
    Status,
    /// Show cached context (for AI assistants)
    Show,
    /// Show cache file path
    Path,
}

#[derive(Subcommand, Debug)]
pub enum EvalCommands {
    /// Run evaluation on a scenario's call log
    Run {
        /// Path to scenario directory (containing scenario.toml and call_log.jsonl)
        #[arg(required = true)]
        scenario: PathBuf,

        /// Minimum score percentage required for success (for CI)
        #[arg(long, default_value_t = 0)]
        min_score: u8,

        /// Require all expected outcomes to pass
        #[arg(long)]
        strict: bool,
    },
    /// Run all scenarios in a directory and report results
    RunAll {
        /// Path to fixtures directory (default: ./fixtures/scenarios)
        #[arg(long, default_value = "./fixtures/scenarios")]
        path: PathBuf,

        /// Minimum score percentage required for each scenario
        #[arg(long, default_value_t = 70)]
        min_score: u8,

        /// Stop on first failure
        #[arg(long)]
        fail_fast: bool,
    },
    /// List available scenarios
    List {
        /// Path to fixtures directory (default: ./fixtures/scenarios)
        #[arg(long, default_value = "./fixtures/scenarios")]
        path: PathBuf,
    },
    /// Show scenario details and prompt
    Show {
        /// Path to scenario directory
        #[arg(required = true)]
        scenario: PathBuf,
    },
    /// Clear the call log for a scenario (prepare for new evaluation)
    Clear {
        /// Path to scenario directory
        #[arg(required = true)]
        scenario: PathBuf,
    },
    /// Clear all call logs in a directory
    ClearAll {
        /// Path to fixtures directory (default: ./fixtures/scenarios)
        #[arg(long, default_value = "./fixtures/scenarios")]
        path: PathBuf,
    },
    /// Check mock mode status and environment
    Status,
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
        #[arg(
            long,
            short = 's',
            required_unless_present = "json",
            conflicts_with = "json"
        )]
        summary: Option<String>,
        /// Issue description
        #[arg(long, short = 'd', conflicts_with = "json", allow_hyphen_values = true)]
        description: Option<String>,
        /// Read body text from a file ("-" for stdin), sets description
        #[arg(long, value_name = "PATH", conflicts_with_all = ["description", "json"])]
        body_file: Option<PathBuf>,
        /// Custom field value (format: FIELD=VALUE, can be repeated)
        #[arg(
            long = "field",
            short = 'f',
            value_name = "FIELD=VALUE",
            conflicts_with = "json"
        )]
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
        /// Validate custom fields against project schema before creating
        #[arg(long)]
        validate: bool,
        /// Validate only, do not create the issue (requires --validate)
        #[arg(long, requires = "validate")]
        dry_run: bool,
        /// JSON payload for issue creation
        #[arg(long, conflicts_with_all = ["project", "summary", "description", "body_file", "fields", "state", "priority", "assignee", "tags", "parent", "validate", "dry_run"], value_name = "JSON")]
        json: Option<String>,
    },
    /// Update existing issue(s) - supports comma-separated IDs for batch updates
    #[command(visible_alias = "u", group(
        ArgGroup::new("update_fields")
            .args(["summary", "description", "body_file", "fields", "state", "priority", "assignee", "tags", "parent", "json"])
            .required(true)
            .multiple(true)
    ))]
    Update {
        /// Issue ID(s) - comma-separated for batch updates (e.g., PROJ-123 or PROJ-1,PROJ-2,PROJ-3)
        #[arg(value_delimiter = ',')]
        ids: Vec<String>,
        /// New summary
        #[arg(long, short = 's')]
        summary: Option<String>,
        /// New description
        #[arg(long, short = 'd', allow_hyphen_values = true)]
        description: Option<String>,
        /// Read body text from a file ("-" for stdin), sets description
        #[arg(long, value_name = "PATH", conflicts_with_all = ["description", "json"])]
        body_file: Option<PathBuf>,
        /// Custom field value (format: FIELD=VALUE, can be repeated)
        #[arg(
            long = "field",
            short = 'f',
            value_name = "FIELD=VALUE",
            conflicts_with = "json"
        )]
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
        /// Parent issue ID to set on this issue (e.g., PROJ-123)
        #[arg(long, conflicts_with = "json")]
        parent: Option<String>,
        /// Validate custom fields against project schema before updating
        #[arg(long)]
        validate: bool,
        /// Validate only, do not update the issue (requires --validate)
        #[arg(long, requires = "validate")]
        dry_run: bool,
        /// JSON payload for issue update
        #[arg(long, conflicts_with_all = ["summary", "description", "body_file", "fields", "state", "priority", "assignee", "tags", "parent", "validate", "dry_run"], value_name = "JSON")]
        json: Option<String>,
    },
    /// Search issues
    #[command(visible_alias = "s", visible_alias = "find")]
    Search {
        /// Search query (e.g., "project: MyProject #Unresolved")
        #[arg(required_unless_present = "template")]
        query: Option<String>,

        /// Use a pre-built query template (see: track cache show for available templates)
        #[arg(long, short = 'T', conflicts_with = "query")]
        template: Option<String>,

        /// Project for template substitution (replaces {PROJECT} in template)
        #[arg(long, short = 'p')]
        project: Option<String>,

        /// Maximum number of results
        #[arg(long, default_value_t = 20, conflicts_with = "all")]
        limit: usize,
        /// Number of results to skip
        #[arg(long, default_value_t = 0, conflicts_with = "all")]
        skip: usize,
        /// Fetch all results (paginate automatically)
        #[arg(long)]
        all: bool,
    },
    /// Delete issue(s) by ID - supports comma-separated IDs for batch deletion
    #[command(visible_alias = "rm", visible_alias = "del")]
    Delete {
        /// Issue ID(s) - comma-separated for batch deletion (e.g., PROJ-123 or PROJ-1,PROJ-2,PROJ-3)
        #[arg(value_delimiter = ',')]
        ids: Vec<String>,
    },
    /// Add a comment to an issue
    #[command(visible_alias = "cmt")]
    Comment {
        /// Issue ID (e.g., PROJ-123)
        id: String,
        /// Comment text
        #[arg(short = 'm', long = "message", required_unless_present = "body_file")]
        text: Option<String>,
        /// Read comment text from a file ("-" for stdin)
        #[arg(long, value_name = "PATH", conflicts_with = "text")]
        body_file: Option<PathBuf>,
    },
    /// List comments on an issue
    Comments {
        /// Issue ID (e.g., PROJ-123)
        id: String,
        /// Maximum number of comments to show
        #[arg(long, default_value_t = 10, conflicts_with = "all")]
        limit: usize,
        /// Fetch all comments (paginate automatically)
        #[arg(long)]
        all: bool,
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
    /// Remove a link between issues
    #[command(visible_alias = "ul")]
    Unlink {
        /// Source issue ID (e.g., PROJ-123)
        source: String,
        /// Link ID to remove (from `issue get --full` or `issue links` output)
        link_id: String,
    },
    /// Start work on issue(s) (set state to in-progress) - supports comma-separated IDs
    Start {
        /// Issue ID(s) - comma-separated for batch (e.g., PROJ-123 or PROJ-1,PROJ-2,PROJ-3)
        #[arg(value_delimiter = ',')]
        ids: Vec<String>,
        /// State field name (default: "Stage")
        #[arg(long, default_value = "Stage")]
        field: String,
        /// State value for in-progress (default: "Develop")
        #[arg(long, default_value = "Develop")]
        state: String,
    },
    /// Complete issue(s) (set state to done/resolved) - supports comma-separated IDs
    #[command(visible_alias = "done", visible_alias = "resolve")]
    Complete {
        /// Issue ID(s) - comma-separated for batch (e.g., PROJ-123 or PROJ-1,PROJ-2,PROJ-3)
        #[arg(value_delimiter = ',')]
        ids: Vec<String>,
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
    /// Create a new project
    #[command(visible_alias = "new", visible_alias = "c")]
    Create {
        /// Human-readable project name
        #[arg(long, short = 'n')]
        name: String,
        /// Short name / project key (e.g., "PROJ" for PROJ-123 issues)
        #[arg(long, short = 's')]
        short_name: String,
        /// Project description
        #[arg(long, short = 'd', allow_hyphen_values = true)]
        description: Option<String>,
        /// Read body text from a file ("-" for stdin), sets description
        #[arg(long, value_name = "PATH", conflicts_with = "description")]
        body_file: Option<PathBuf>,
    },
    /// List custom fields for a project
    #[command(visible_alias = "f")]
    Fields {
        /// Project ID or short name (e.g., "OGIT" or "0-2")
        id: String,
    },
    /// Attach a custom field to a project
    AttachField {
        /// Project ID or short name
        project: String,
        /// Field ID to attach
        #[arg(long, short = 'f', required = true)]
        field: String,
        /// Bundle ID (required for enum/state fields)
        #[arg(long)]
        bundle: Option<String>,
        /// Make field required (cannot be empty)
        #[arg(long)]
        required: bool,
        /// Text to show when field is empty
        #[arg(long)]
        empty_text: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum FieldCommands {
    /// List all custom field definitions
    #[command(visible_alias = "ls")]
    List,
    /// Create a new custom field definition
    #[command(visible_alias = "c")]
    Create {
        /// Field name
        name: String,
        /// Field type: enum, multi-enum, state, text, date, integer, float, period
        #[arg(long, short = 't', default_value = "enum")]
        field_type: String,
    },
    /// Create a field with values and attach to project in one step
    #[command(visible_alias = "setup")]
    New {
        /// Field name
        name: String,
        /// Field type: enum, state
        #[arg(long, short = 't', default_value = "enum")]
        field_type: String,
        /// Project to attach to
        #[arg(long, short = 'p', required = true)]
        project: String,
        /// Comma-separated values for the field
        #[arg(long, value_delimiter = ',', required = true)]
        values: Vec<String>,
        /// Value(s) that represent resolved state (for state fields, comma-separated)
        #[arg(long, value_delimiter = ',')]
        resolved: Vec<String>,
        /// Make the field required (cannot be empty)
        #[arg(long)]
        required: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum BundleCommands {
    /// List bundles by type
    #[command(visible_alias = "ls")]
    List {
        /// Bundle type: enum, state, ownedField, version, build
        #[arg(long, short = 't', default_value = "enum")]
        bundle_type: String,
    },
    /// Create a new bundle with optional initial values
    #[command(visible_alias = "c")]
    Create {
        /// Bundle name
        name: String,
        /// Bundle type: enum, state
        #[arg(long, short = 't', default_value = "enum")]
        bundle_type: String,
        /// Initial values (comma-separated)
        #[arg(long, value_delimiter = ',')]
        values: Vec<String>,
        /// Value(s) that represent resolved state (for state bundles, comma-separated)
        #[arg(long, value_delimiter = ',')]
        resolved: Vec<String>,
    },
    /// Add a value to an existing bundle
    AddValue {
        /// Bundle ID
        bundle_id: String,
        /// Bundle type: enum, state
        #[arg(long, short = 't', required = true)]
        bundle_type: String,
        /// Value name to add
        #[arg(long, required = true)]
        value: String,
        /// Mark this value as resolved (for state bundles)
        #[arg(long)]
        resolved: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum TagCommands {
    /// List all available tags
    #[command(visible_alias = "ls")]
    List,
    /// Create a new tag/label
    #[command(visible_alias = "c")]
    Create {
        /// Tag name
        name: String,
        /// Color hex string (e.g., "#d73a4a" or "d73a4a")
        #[arg(long = "tag-color")]
        tag_color: Option<String>,
        /// Description
        #[arg(long, short = 'd', allow_hyphen_values = true)]
        description: Option<String>,
        /// Read body text from a file ("-" for stdin), sets description
        #[arg(long, value_name = "PATH", conflicts_with = "description")]
        body_file: Option<PathBuf>,
    },
    /// Delete a tag/label
    #[command(visible_alias = "rm")]
    Delete {
        /// Tag name to delete
        name: String,
    },
    /// Update a tag/label
    #[command(visible_alias = "u")]
    Update {
        /// Current tag name
        name: String,
        /// New name (rename)
        #[arg(long)]
        new_name: Option<String>,
        /// New color hex string
        #[arg(long = "tag-color")]
        tag_color: Option<String>,
        /// New description
        #[arg(long, short = 'd', allow_hyphen_values = true)]
        description: Option<String>,
        /// Read body text from a file ("-" for stdin), sets description
        #[arg(long, value_name = "PATH", conflicts_with = "description")]
        body_file: Option<PathBuf>,
    },
}

#[derive(Subcommand, Debug)]
pub enum ArticleCommands {
    /// Get article by ID
    #[command(visible_alias = "g")]
    Get {
        /// Article ID (e.g., PROJ-A-1 or database ID)
        id: String,
    },
    /// List articles
    #[command(visible_alias = "ls")]
    List {
        /// Filter by project ID or shortName
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Maximum number of results
        #[arg(long, default_value_t = 20, conflicts_with = "all")]
        limit: usize,
        /// Number of results to skip
        #[arg(long, default_value_t = 0, conflicts_with = "all")]
        skip: usize,
        /// Fetch all articles (paginate automatically)
        #[arg(long)]
        all: bool,
    },
    /// Search articles
    #[command(visible_alias = "s", visible_alias = "find")]
    Search {
        /// Search query
        query: String,
        /// Maximum number of results
        #[arg(long, default_value_t = 20, conflicts_with = "all")]
        limit: usize,
        /// Number of results to skip
        #[arg(long, default_value_t = 0, conflicts_with = "all")]
        skip: usize,
        /// Fetch all results (paginate automatically)
        #[arg(long)]
        all: bool,
    },
    /// Create new article
    #[command(visible_alias = "new", visible_alias = "c")]
    Create {
        /// Project ID or shortName
        #[arg(long, short = 'p', required = true)]
        project: String,
        /// Article title
        #[arg(long, short = 's', required = true)]
        summary: String,
        /// Article content (Markdown)
        #[arg(long, short = 'c')]
        content: Option<String>,
        /// Read body text from a file ("-" for stdin), sets content
        #[arg(long, value_name = "PATH", conflicts_with_all = ["content", "content_file"])]
        body_file: Option<PathBuf>,
        /// Read content from file (hidden, use --body-file instead)
        #[arg(long, conflicts_with_all = ["content", "body_file"], hide = true)]
        content_file: Option<PathBuf>,
        /// Parent article ID (for creating child articles)
        #[arg(long)]
        parent: Option<String>,
        /// Tag name (can be repeated)
        #[arg(long = "tag", short = 't')]
        tags: Vec<String>,
    },
    /// Update existing article
    #[command(visible_alias = "u")]
    Update {
        /// Article ID
        id: String,
        /// New title
        #[arg(long, short = 's')]
        summary: Option<String>,
        /// New content (Markdown)
        #[arg(long, short = 'c')]
        content: Option<String>,
        /// Read body text from a file ("-" for stdin), sets content
        #[arg(long, value_name = "PATH", conflicts_with_all = ["content", "content_file"])]
        body_file: Option<PathBuf>,
        /// Read content from file (hidden, use --body-file instead)
        #[arg(long, conflicts_with_all = ["content", "body_file"], hide = true)]
        content_file: Option<PathBuf>,
        /// Tag name (can be repeated)
        #[arg(long = "tag", short = 't')]
        tags: Vec<String>,
    },
    /// Delete article by ID
    #[command(visible_alias = "rm", visible_alias = "del")]
    Delete {
        /// Article ID
        id: String,
    },
    /// Show article hierarchy (children)
    Tree {
        /// Article ID to show children for
        id: String,
    },
    /// Move article to new parent
    Move {
        /// Article ID to move
        id: String,
        /// New parent article ID (omit to move to root)
        #[arg(long)]
        parent: Option<String>,
    },
    /// List attachments on an article
    Attachments {
        /// Article ID
        id: String,
    },
    /// Add a comment to an article
    #[command(visible_alias = "cmt")]
    Comment {
        /// Article ID
        id: String,
        /// Comment text
        #[arg(short = 'm', long = "message", required_unless_present = "body_file")]
        text: Option<String>,
        /// Read comment text from a file ("-" for stdin)
        #[arg(long, value_name = "PATH", conflicts_with = "text")]
        body_file: Option<PathBuf>,
    },
    /// List comments on an article
    Comments {
        /// Article ID
        id: String,
        /// Maximum number of comments to show
        #[arg(long, default_value_t = 10, conflicts_with = "all")]
        limit: usize,
        /// Fetch all comments (paginate automatically)
        #[arg(long)]
        all: bool,
    },
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
        let cli = Cli::parse_from([
            "track",
            "--config",
            "/tmp/track-config.toml",
            "project",
            "list",
        ]);

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
                IssueCommands::Update { ids, json, .. } => {
                    assert_eq!(ids, vec!["PROJ-1"]);
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
        let cli = Cli::parse_from(["track", "issue", "update", "PROJ-1", "--state", "Resolved"]);

        match cli.command {
            Commands::Issue { action } => match action {
                IssueCommands::Update { ids, state, .. } => {
                    assert_eq!(ids, vec!["PROJ-1"]);
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
    fn parses_update_with_parent_flag() {
        let cli = Cli::parse_from(["track", "issue", "update", "PROJ-1", "--parent", "PROJ-100"]);

        match cli.command {
            Commands::Issue { action } => match action {
                IssueCommands::Update {
                    ids,
                    parent,
                    summary,
                    ..
                } => {
                    assert_eq!(ids, vec!["PROJ-1"]);
                    assert_eq!(parent.as_deref(), Some("PROJ-100"));
                    assert!(summary.is_none());
                }
                _ => panic!("expected issue update"),
            },
            _ => panic!("expected issue command"),
        }
    }

    #[test]
    fn rejects_update_json_with_parent_flag() {
        let result = Cli::try_parse_from([
            "track",
            "issue",
            "update",
            "PROJ-1",
            "--json",
            r#"{"summary":"X"}"#,
            "--parent",
            "PROJ-100",
        ]);
        assert!(result.is_err());
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
                IssueCommands::Comment { id, text, .. } => {
                    assert_eq!(id, "PROJ-123");
                    assert_eq!(text.as_deref(), Some("This is a comment"));
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
                IssueCommands::Comments { id, limit, all } => {
                    assert_eq!(id, "PROJ-123");
                    assert_eq!(limit, 5);
                    assert!(!all);
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
    fn parses_unlink_command() {
        let cli = Cli::parse_from(["track", "issue", "unlink", "PROJ-123", "142-3t/PROJ-456"]);

        match cli.command {
            Commands::Issue { action } => match action {
                IssueCommands::Unlink { source, link_id } => {
                    assert_eq!(source, "PROJ-123");
                    assert_eq!(link_id, "142-3t/PROJ-456");
                }
                _ => panic!("expected issue unlink"),
            },
            _ => panic!("expected issue command"),
        }
    }

    #[test]
    fn parses_unlink_alias() {
        let cli = Cli::parse_from(["track", "issue", "ul", "PROJ-123", "abc123"]);

        match cli.command {
            Commands::Issue { action } => match action {
                IssueCommands::Unlink { source, link_id } => {
                    assert_eq!(source, "PROJ-123");
                    assert_eq!(link_id, "abc123");
                }
                _ => panic!("expected issue unlink via alias"),
            },
            _ => panic!("expected issue command"),
        }
    }

    #[test]
    fn parses_start_command_with_defaults() {
        let cli = Cli::parse_from(["track", "issue", "start", "PROJ-123"]);

        match cli.command {
            Commands::Issue { action } => match action {
                IssueCommands::Start { ids, field, state } => {
                    assert_eq!(ids, vec!["PROJ-123"]);
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
            "track", "issue", "complete", "PROJ-123", "--field", "State", "--state", "Resolved",
        ]);

        match cli.command {
            Commands::Issue { action } => match action {
                IssueCommands::Complete { ids, field, state } => {
                    assert_eq!(ids, vec!["PROJ-123"]);
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
                IssueCommands::Complete { ids, .. } => {
                    assert_eq!(ids, vec!["PROJ-123"]);
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

    #[test]
    fn parses_init_command() {
        let cli = Cli::parse_from([
            "track",
            "init",
            "--url",
            "https://youtrack.example.com",
            "--token",
            "perm:xxx",
        ]);

        match cli.command {
            Commands::Init {
                url,
                token,
                project,
                backend,
                email,
                skills,
                global,
            } => {
                assert_eq!(url.as_deref(), Some("https://youtrack.example.com"));
                assert_eq!(token.as_deref(), Some("perm:xxx"));
                assert!(project.is_none());
                assert!(matches!(backend, Backend::YouTrack)); // Default
                assert!(email.is_none());
                assert!(!skills);
                assert!(!global);
            }
            _ => panic!("expected init command"),
        }
    }

    #[test]
    fn parses_init_command_with_project() {
        let cli = Cli::parse_from([
            "track",
            "init",
            "--url",
            "https://youtrack.example.com",
            "--token",
            "perm:xxx",
            "--project",
            "PROJ",
        ]);

        match cli.command {
            Commands::Init {
                url,
                token,
                project,
                backend,
                ..
            } => {
                assert_eq!(url.as_deref(), Some("https://youtrack.example.com"));
                assert_eq!(token.as_deref(), Some("perm:xxx"));
                assert_eq!(project.as_deref(), Some("PROJ"));
                assert!(matches!(backend, Backend::YouTrack));
            }
            _ => panic!("expected init command"),
        }
    }

    #[test]
    fn parses_init_command_with_jira_backend() {
        let cli = Cli::parse_from([
            "track",
            "init",
            "--url",
            "https://example.atlassian.net",
            "--token",
            "api-token",
            "--backend",
            "jira",
            "--email",
            "test@example.com",
        ]);

        match cli.command {
            Commands::Init {
                url,
                token,
                backend,
                email,
                ..
            } => {
                assert_eq!(url.as_deref(), Some("https://example.atlassian.net"));
                assert_eq!(token.as_deref(), Some("api-token"));
                assert!(matches!(backend, Backend::Jira));
                assert_eq!(email.as_deref(), Some("test@example.com"));
            }
            _ => panic!("expected init command"),
        }
    }

    #[test]
    fn parses_init_command_with_github_project() {
        let cli = Cli::parse_from([
            "track",
            "init",
            "--url",
            "https://api.github.com",
            "--token",
            "ghp-token",
            "--backend",
            "github",
            "--project",
            "owner/repo",
        ]);

        match cli.command {
            Commands::Init {
                url,
                token,
                backend,
                project,
                ..
            } => {
                assert_eq!(url.as_deref(), Some("https://api.github.com"));
                assert_eq!(token.as_deref(), Some("ghp-token"));
                assert!(matches!(backend, Backend::GitHub));
                assert_eq!(project.as_deref(), Some("owner/repo"));
            }
            _ => panic!("expected init command"),
        }
    }

    #[test]
    fn parses_init_skills_only() {
        let cli = Cli::parse_from(["track", "init", "--skills"]);

        match cli.command {
            Commands::Init {
                url, token, skills, ..
            } => {
                assert!(url.is_none());
                assert!(token.is_none());
                assert!(skills);
            }
            _ => panic!("expected init command"),
        }
    }

    #[test]
    fn rejects_init_without_url() {
        let result = Cli::try_parse_from(["track", "init", "--token", "perm:xxx"]);
        assert!(result.is_err());
    }

    #[test]
    fn rejects_init_without_token() {
        let result = Cli::try_parse_from(["track", "init", "--url", "https://example.com"]);
        assert!(result.is_err());
    }

    #[test]
    fn parses_open_command_with_issue_id() {
        let cli = Cli::parse_from(["track", "open", "PROJ-123"]);

        match cli.command {
            Commands::Open { id } => {
                assert_eq!(id.as_deref(), Some("PROJ-123"));
            }
            _ => panic!("expected open command"),
        }
    }

    #[test]
    fn parses_open_command_without_id() {
        let cli = Cli::parse_from(["track", "open"]);

        match cli.command {
            Commands::Open { id } => {
                assert!(id.is_none());
            }
            _ => panic!("expected open command"),
        }
    }

    #[test]
    fn parses_config_test_command() {
        let cli = Cli::parse_from(["track", "config", "test"]);

        match cli.command {
            Commands::Config { action } => match action {
                ConfigCommands::Test => {}
                _ => panic!("expected config test"),
            },
            _ => panic!("expected config command"),
        }
    }

    #[test]
    fn parses_config_backend_command() {
        let cli = Cli::parse_from(["track", "config", "backend", "jira"]);

        match cli.command {
            Commands::Config { action } => match action {
                ConfigCommands::Backend { backend } => {
                    assert!(matches!(backend, Backend::Jira));
                }
                _ => panic!("expected config backend"),
            },
            _ => panic!("expected config command"),
        }
    }

    #[test]
    fn parses_config_backend_with_alias() {
        let cli = Cli::parse_from(["track", "config", "backend", "yt"]);

        match cli.command {
            Commands::Config { action } => match action {
                ConfigCommands::Backend { backend } => {
                    assert!(matches!(backend, Backend::YouTrack));
                }
                _ => panic!("expected config backend"),
            },
            _ => panic!("expected config command"),
        }
    }

    #[test]
    fn parses_external_subcommand_as_issue_shortcut() {
        let cli = Cli::parse_from(["track", "PROJ-123"]);

        match cli.command {
            Commands::External(args) => {
                assert_eq!(args, vec!["PROJ-123"]);
            }
            _ => panic!("expected external subcommand"),
        }
    }

    #[test]
    fn parses_config_set_command() {
        let cli = Cli::parse_from(["track", "config", "set", "jira.email", "test@example.com"]);

        match cli.command {
            Commands::Config { action } => match action {
                ConfigCommands::Set { key, value, global } => {
                    assert_eq!(key, "jira.email");
                    assert_eq!(value, "test@example.com");
                    assert!(!global);
                }
                _ => panic!("expected config set"),
            },
            _ => panic!("expected config command"),
        }
    }

    #[test]
    fn parses_config_get_command() {
        let cli = Cli::parse_from(["track", "config", "get", "backend"]);

        match cli.command {
            Commands::Config { action } => match action {
                ConfigCommands::Get { key } => {
                    assert_eq!(key, "backend");
                }
                _ => panic!("expected config get"),
            },
            _ => panic!("expected config command"),
        }
    }

    #[test]
    fn parses_config_keys_command() {
        let cli = Cli::parse_from(["track", "config", "keys"]);

        match cli.command {
            Commands::Config { action } => {
                assert!(matches!(action, ConfigCommands::Keys));
            }
            _ => panic!("expected config command"),
        }
    }

    #[test]
    fn parses_context_command() {
        let cli = Cli::parse_from(["track", "context"]);

        match cli.command {
            Commands::Context {
                project,
                refresh,
                include_issues,
                issue_limit,
            } => {
                assert!(project.is_none());
                assert!(!refresh);
                assert!(!include_issues);
                assert_eq!(issue_limit, 10);
            }
            _ => panic!("expected context command"),
        }
    }

    #[test]
    fn parses_context_command_with_flags() {
        let cli = Cli::parse_from([
            "track",
            "context",
            "--project",
            "PROJ",
            "--refresh",
            "--include-issues",
            "--issue-limit",
            "25",
        ]);

        match cli.command {
            Commands::Context {
                project,
                refresh,
                include_issues,
                issue_limit,
            } => {
                assert_eq!(project.as_deref(), Some("PROJ"));
                assert!(refresh);
                assert!(include_issues);
                assert_eq!(issue_limit, 25);
            }
            _ => panic!("expected context command"),
        }
    }

    #[test]
    fn parses_context_alias() {
        let cli = Cli::parse_from(["track", "ctx"]);

        assert!(matches!(cli.command, Commands::Context { .. }));
    }

    #[test]
    fn parses_search_with_query() {
        let cli = Cli::parse_from(["track", "issue", "search", "project: PROJ #Unresolved"]);

        match cli.command {
            Commands::Issue { action } => match action {
                IssueCommands::Search {
                    query,
                    template,
                    project,
                    limit,
                    skip,
                    all,
                } => {
                    assert_eq!(query.as_deref(), Some("project: PROJ #Unresolved"));
                    assert!(template.is_none());
                    assert!(project.is_none());
                    assert_eq!(limit, 20);
                    assert_eq!(skip, 0);
                    assert!(!all);
                }
                _ => panic!("expected issue search"),
            },
            _ => panic!("expected issue command"),
        }
    }

    #[test]
    fn parses_search_with_template() {
        let cli = Cli::parse_from([
            "track",
            "issue",
            "search",
            "--template",
            "unresolved",
            "--project",
            "PROJ",
        ]);

        match cli.command {
            Commands::Issue { action } => match action {
                IssueCommands::Search {
                    query,
                    template,
                    project,
                    ..
                } => {
                    assert!(query.is_none());
                    assert_eq!(template.as_deref(), Some("unresolved"));
                    assert_eq!(project.as_deref(), Some("PROJ"));
                }
                _ => panic!("expected issue search"),
            },
            _ => panic!("expected issue command"),
        }
    }

    #[test]
    fn parses_search_with_template_short_flags() {
        let cli = Cli::parse_from(["track", "i", "s", "-T", "my_issues", "-p", "PROJ"]);

        match cli.command {
            Commands::Issue { action } => match action {
                IssueCommands::Search {
                    query,
                    template,
                    project,
                    ..
                } => {
                    assert!(query.is_none());
                    assert_eq!(template.as_deref(), Some("my_issues"));
                    assert_eq!(project.as_deref(), Some("PROJ"));
                }
                _ => panic!("expected issue search"),
            },
            _ => panic!("expected issue command"),
        }
    }

    #[test]
    fn rejects_search_with_both_query_and_template() {
        let result = Cli::try_parse_from([
            "track",
            "issue",
            "search",
            "some query",
            "--template",
            "unresolved",
        ]);

        assert!(result.is_err());
    }

    #[test]
    fn parses_create_with_validate_flag() {
        let cli = Cli::parse_from([
            "track",
            "issue",
            "create",
            "-p",
            "PROJ",
            "-s",
            "Test",
            "--validate",
        ]);

        match cli.command {
            Commands::Issue { action } => match action {
                IssueCommands::Create {
                    validate, dry_run, ..
                } => {
                    assert!(validate);
                    assert!(!dry_run);
                }
                _ => panic!("expected issue create"),
            },
            _ => panic!("expected issue command"),
        }
    }

    #[test]
    fn parses_create_with_validate_and_dry_run() {
        let cli = Cli::parse_from([
            "track",
            "issue",
            "create",
            "-p",
            "PROJ",
            "-s",
            "Test",
            "--validate",
            "--dry-run",
        ]);

        match cli.command {
            Commands::Issue { action } => match action {
                IssueCommands::Create {
                    validate, dry_run, ..
                } => {
                    assert!(validate);
                    assert!(dry_run);
                }
                _ => panic!("expected issue create"),
            },
            _ => panic!("expected issue command"),
        }
    }

    #[test]
    fn rejects_dry_run_without_validate() {
        let result = Cli::try_parse_from([
            "track",
            "issue",
            "create",
            "-p",
            "PROJ",
            "-s",
            "Test",
            "--dry-run",
        ]);

        assert!(result.is_err());
    }

    #[test]
    fn parses_update_with_validate_flag() {
        let cli = Cli::parse_from([
            "track",
            "issue",
            "update",
            "PROJ-123",
            "--field",
            "Priority=Major",
            "--validate",
        ]);

        match cli.command {
            Commands::Issue { action } => match action {
                IssueCommands::Update {
                    validate, dry_run, ..
                } => {
                    assert!(validate);
                    assert!(!dry_run);
                }
                _ => panic!("expected issue update"),
            },
            _ => panic!("expected issue command"),
        }
    }

    #[test]
    fn parses_update_with_validate_and_dry_run() {
        let cli = Cli::parse_from([
            "track",
            "issue",
            "update",
            "PROJ-123",
            "--field",
            "State=Done",
            "--validate",
            "--dry-run",
        ]);

        match cli.command {
            Commands::Issue { action } => match action {
                IssueCommands::Update {
                    validate, dry_run, ..
                } => {
                    assert!(validate);
                    assert!(dry_run);
                }
                _ => panic!("expected issue update"),
            },
            _ => panic!("expected issue command"),
        }
    }

    #[test]
    fn parses_cache_refresh_command() {
        let cli = Cli::parse_from(["track", "cache", "refresh"]);

        match cli.command {
            Commands::Cache { action } => match action {
                CacheCommands::Refresh { if_stale } => {
                    assert!(if_stale.is_none());
                }
                _ => panic!("expected cache refresh"),
            },
            _ => panic!("expected cache command"),
        }
    }

    #[test]
    fn parses_cache_refresh_with_if_stale() {
        let cli = Cli::parse_from(["track", "cache", "refresh", "--if-stale", "1h"]);

        match cli.command {
            Commands::Cache { action } => match action {
                CacheCommands::Refresh { if_stale } => {
                    assert_eq!(if_stale.as_deref(), Some("1h"));
                }
                _ => panic!("expected cache refresh"),
            },
            _ => panic!("expected cache command"),
        }
    }

    #[test]
    fn parses_cache_status_command() {
        let cli = Cli::parse_from(["track", "cache", "status"]);

        match cli.command {
            Commands::Cache { action } => {
                assert!(matches!(action, CacheCommands::Status));
            }
            _ => panic!("expected cache command"),
        }
    }

    // =========================================================================
    // Batch Operations Tests
    // =========================================================================

    #[test]
    fn parses_update_with_multiple_ids() {
        let cli = Cli::parse_from([
            "track",
            "issue",
            "update",
            "PROJ-1,PROJ-2,PROJ-3",
            "--field",
            "Priority=Major",
        ]);

        match cli.command {
            Commands::Issue { action } => match action {
                IssueCommands::Update { ids, fields, .. } => {
                    assert_eq!(ids, vec!["PROJ-1", "PROJ-2", "PROJ-3"]);
                    assert_eq!(fields, vec!["Priority=Major"]);
                }
                _ => panic!("expected issue update"),
            },
            _ => panic!("expected issue command"),
        }
    }

    #[test]
    fn parses_delete_with_multiple_ids() {
        let cli = Cli::parse_from(["track", "issue", "delete", "PROJ-1,PROJ-2"]);

        match cli.command {
            Commands::Issue { action } => match action {
                IssueCommands::Delete { ids } => {
                    assert_eq!(ids, vec!["PROJ-1", "PROJ-2"]);
                }
                _ => panic!("expected issue delete"),
            },
            _ => panic!("expected issue command"),
        }
    }

    #[test]
    fn parses_start_with_multiple_ids() {
        let cli = Cli::parse_from(["track", "issue", "start", "PROJ-1,PROJ-2,PROJ-3"]);

        match cli.command {
            Commands::Issue { action } => match action {
                IssueCommands::Start { ids, field, state } => {
                    assert_eq!(ids, vec!["PROJ-1", "PROJ-2", "PROJ-3"]);
                    assert_eq!(field, "Stage");
                    assert_eq!(state, "Develop");
                }
                _ => panic!("expected issue start"),
            },
            _ => panic!("expected issue command"),
        }
    }

    #[test]
    fn parses_complete_with_multiple_ids() {
        let cli = Cli::parse_from([
            "track",
            "issue",
            "complete",
            "PROJ-1,PROJ-2",
            "--state",
            "Done",
        ]);

        match cli.command {
            Commands::Issue { action } => match action {
                IssueCommands::Complete { ids, state, .. } => {
                    assert_eq!(ids, vec!["PROJ-1", "PROJ-2"]);
                    assert_eq!(state, "Done");
                }
                _ => panic!("expected issue complete"),
            },
            _ => panic!("expected issue command"),
        }
    }

    #[test]
    fn parses_single_id_in_batch_commands() {
        // Single ID should work the same way - stored in a Vec with one element
        let cli = Cli::parse_from([
            "track",
            "issue",
            "update",
            "PROJ-1",
            "--field",
            "Priority=Major",
        ]);

        match cli.command {
            Commands::Issue { action } => match action {
                IssueCommands::Update { ids, .. } => {
                    assert_eq!(ids, vec!["PROJ-1"]);
                }
                _ => panic!("expected issue update"),
            },
            _ => panic!("expected issue command"),
        }
    }

    // =========================================================================
    // --body-file Tests
    // =========================================================================

    #[test]
    fn parses_issue_create_with_body_file() {
        let cli = Cli::parse_from([
            "track",
            "issue",
            "create",
            "-p",
            "PROJ",
            "-s",
            "Title",
            "--body-file",
            "/tmp/desc.md",
        ]);

        match cli.command {
            Commands::Issue { action } => match action {
                IssueCommands::Create {
                    body_file,
                    description,
                    ..
                } => {
                    assert_eq!(body_file, Some(PathBuf::from("/tmp/desc.md")));
                    assert!(description.is_none());
                }
                _ => panic!("expected issue create"),
            },
            _ => panic!("expected issue command"),
        }
    }

    #[test]
    fn parses_issue_update_with_body_file() {
        let cli = Cli::parse_from([
            "track",
            "issue",
            "update",
            "PROJ-1",
            "--body-file",
            "desc.md",
        ]);

        match cli.command {
            Commands::Issue { action } => match action {
                IssueCommands::Update {
                    ids,
                    body_file,
                    description,
                    ..
                } => {
                    assert_eq!(ids, vec!["PROJ-1"]);
                    assert_eq!(body_file, Some(PathBuf::from("desc.md")));
                    assert!(description.is_none());
                }
                _ => panic!("expected issue update"),
            },
            _ => panic!("expected issue command"),
        }
    }

    #[test]
    fn parses_issue_update_body_file_satisfies_required_group() {
        // --body-file alone should satisfy the update_fields requirement
        let result = Cli::try_parse_from([
            "track",
            "issue",
            "update",
            "PROJ-1",
            "--body-file",
            "desc.md",
        ]);
        assert!(
            result.is_ok(),
            "body-file should satisfy update_fields group"
        );
    }

    #[test]
    fn rejects_issue_create_body_file_with_description() {
        let result = Cli::try_parse_from([
            "track",
            "issue",
            "create",
            "-p",
            "PROJ",
            "-s",
            "Title",
            "--description",
            "inline",
            "--body-file",
            "file.md",
        ]);
        assert!(result.is_err(), "body-file and description should conflict");
    }

    #[test]
    fn rejects_issue_create_body_file_with_json() {
        let result = Cli::try_parse_from([
            "track",
            "issue",
            "create",
            "--json",
            "{\"summary\":\"Test\"}",
            "--body-file",
            "file.md",
        ]);
        assert!(result.is_err(), "body-file and json should conflict");
    }

    #[test]
    fn rejects_issue_update_body_file_with_description() {
        let result = Cli::try_parse_from([
            "track",
            "issue",
            "update",
            "PROJ-1",
            "-d",
            "inline",
            "--body-file",
            "file.md",
        ]);
        assert!(result.is_err(), "body-file and description should conflict");
    }

    #[test]
    fn rejects_issue_update_body_file_with_json() {
        let result = Cli::try_parse_from([
            "track",
            "issue",
            "update",
            "PROJ-1",
            "--json",
            "{\"summary\":\"Test\"}",
            "--body-file",
            "file.md",
        ]);
        assert!(result.is_err(), "body-file and json should conflict");
    }

    #[test]
    fn parses_issue_comment_with_body_file() {
        let cli = Cli::parse_from([
            "track",
            "issue",
            "comment",
            "PROJ-1",
            "--body-file",
            "comment.md",
        ]);

        match cli.command {
            Commands::Issue { action } => match action {
                IssueCommands::Comment {
                    id,
                    text,
                    body_file,
                } => {
                    assert_eq!(id, "PROJ-1");
                    assert!(text.is_none());
                    assert_eq!(body_file, Some(PathBuf::from("comment.md")));
                }
                _ => panic!("expected issue comment"),
            },
            _ => panic!("expected issue command"),
        }
    }

    #[test]
    fn rejects_issue_comment_without_message_or_body_file() {
        let result = Cli::try_parse_from(["track", "issue", "comment", "PROJ-1"]);
        assert!(
            result.is_err(),
            "comment requires either --message or --body-file"
        );
    }

    #[test]
    fn rejects_issue_comment_with_both_message_and_body_file() {
        let result = Cli::try_parse_from([
            "track",
            "issue",
            "comment",
            "PROJ-1",
            "-m",
            "inline",
            "--body-file",
            "file.md",
        ]);
        assert!(result.is_err(), "message and body-file should conflict");
    }

    #[test]
    fn parses_project_create_with_body_file() {
        let cli = Cli::parse_from([
            "track",
            "project",
            "create",
            "-n",
            "My Project",
            "-s",
            "PROJ",
            "--body-file",
            "desc.md",
        ]);

        match cli.command {
            Commands::Project { action } => match action {
                ProjectCommands::Create {
                    body_file,
                    description,
                    ..
                } => {
                    assert_eq!(body_file, Some(PathBuf::from("desc.md")));
                    assert!(description.is_none());
                }
                _ => panic!("expected project create"),
            },
            _ => panic!("expected project command"),
        }
    }

    #[test]
    fn parses_tag_create_with_body_file() {
        let cli = Cli::parse_from(["track", "tags", "create", "bug", "--body-file", "desc.md"]);

        match cli.command {
            Commands::Tags { action } => match action {
                TagCommands::Create {
                    name,
                    body_file,
                    description,
                    ..
                } => {
                    assert_eq!(name, "bug");
                    assert_eq!(body_file, Some(PathBuf::from("desc.md")));
                    assert!(description.is_none());
                }
                _ => panic!("expected tag create"),
            },
            _ => panic!("expected tag command"),
        }
    }

    #[test]
    fn parses_tag_update_with_body_file() {
        let cli = Cli::parse_from(["track", "tags", "update", "bug", "--body-file", "desc.md"]);

        match cli.command {
            Commands::Tags { action } => match action {
                TagCommands::Update {
                    name,
                    body_file,
                    description,
                    ..
                } => {
                    assert_eq!(name, "bug");
                    assert_eq!(body_file, Some(PathBuf::from("desc.md")));
                    assert!(description.is_none());
                }
                _ => panic!("expected tag update"),
            },
            _ => panic!("expected tag command"),
        }
    }

    #[test]
    fn parses_article_create_with_body_file() {
        let cli = Cli::parse_from([
            "track",
            "article",
            "create",
            "-p",
            "PROJ",
            "-s",
            "Title",
            "--body-file",
            "article.md",
        ]);

        match cli.command {
            Commands::Article { action } => match action {
                ArticleCommands::Create {
                    body_file,
                    content,
                    content_file,
                    ..
                } => {
                    assert_eq!(body_file, Some(PathBuf::from("article.md")));
                    assert!(content.is_none());
                    assert!(content_file.is_none());
                }
                _ => panic!("expected article create"),
            },
            _ => panic!("expected article command"),
        }
    }

    #[test]
    fn parses_article_create_with_content_file_backward_compat() {
        let cli = Cli::parse_from([
            "track",
            "article",
            "create",
            "-p",
            "PROJ",
            "-s",
            "Title",
            "--content-file",
            "article.md",
        ]);

        match cli.command {
            Commands::Article { action } => match action {
                ArticleCommands::Create {
                    content_file,
                    body_file,
                    content,
                    ..
                } => {
                    assert_eq!(content_file, Some(PathBuf::from("article.md")));
                    assert!(body_file.is_none());
                    assert!(content.is_none());
                }
                _ => panic!("expected article create"),
            },
            _ => panic!("expected article command"),
        }
    }

    #[test]
    fn rejects_article_create_body_file_with_content() {
        let result = Cli::try_parse_from([
            "track",
            "article",
            "create",
            "-p",
            "PROJ",
            "-s",
            "Title",
            "-c",
            "inline content",
            "--body-file",
            "file.md",
        ]);
        assert!(result.is_err(), "body-file and content should conflict");
    }

    #[test]
    fn rejects_article_create_body_file_with_content_file() {
        let result = Cli::try_parse_from([
            "track",
            "article",
            "create",
            "-p",
            "PROJ",
            "-s",
            "Title",
            "--content-file",
            "old.md",
            "--body-file",
            "new.md",
        ]);
        assert!(
            result.is_err(),
            "body-file and content-file should conflict"
        );
    }

    #[test]
    fn parses_article_comment_with_body_file() {
        let cli = Cli::parse_from([
            "track",
            "article",
            "comment",
            "ART-1",
            "--body-file",
            "comment.md",
        ]);

        match cli.command {
            Commands::Article { action } => match action {
                ArticleCommands::Comment {
                    id,
                    text,
                    body_file,
                } => {
                    assert_eq!(id, "ART-1");
                    assert!(text.is_none());
                    assert_eq!(body_file, Some(PathBuf::from("comment.md")));
                }
                _ => panic!("expected article comment"),
            },
            _ => panic!("expected article command"),
        }
    }

    #[test]
    fn parses_issue_update_body_file_stdin_sentinel() {
        let cli = Cli::parse_from(["track", "issue", "update", "PROJ-1", "--body-file", "-"]);

        match cli.command {
            Commands::Issue { action } => match action {
                IssueCommands::Update { body_file, .. } => {
                    assert_eq!(body_file, Some(PathBuf::from("-")));
                }
                _ => panic!("expected issue update"),
            },
            _ => panic!("expected issue command"),
        }
    }
}
