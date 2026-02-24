use crate::error::{GitHubError, Result};
use chrono::{DateTime, Utc};
use git2::{Cred, RemoteCallbacks, Repository, Signature};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use walkdir::WalkDir;

/// YAML front matter for wiki pages
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct FrontMatter {
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tags: Vec<String>,
}

/// Represents a wiki page on disk
#[derive(Debug, Clone)]
pub struct WikiPage {
    /// Page slug (filename without .md extension)
    pub slug: String,
    /// Page title (from front matter or filename)
    pub title: String,
    /// Markdown content (without front matter)
    pub content: String,
    /// Parent directory (if nested)
    pub parent: Option<String>,
    /// Tags from front matter
    pub tags: Vec<String>,
    /// Creation timestamp (first commit)
    pub created: DateTime<Utc>,
    /// Last update timestamp (last commit)
    pub updated: DateTime<Utc>,
    /// Author (from last commit)
    pub author: Option<String>,
}

/// Validate a slug to prevent path traversal attacks
pub(crate) fn validate_slug(slug: &str) -> Result<()> {
    if slug.is_empty()
        || slug.contains("..")
        || slug.starts_with('/')
        || slug.starts_with('\\')
        || slug.contains('\0')
    {
        return Err(GitHubError::Wiki(format!("Invalid slug: '{}'", slug)));
    }
    Ok(())
}

/// Manages a GitHub wiki as a Git repository
pub struct WikiManager {
    owner: String,
    repo: String,
    token: String,
    cache_dir: PathBuf,
    initialized: Mutex<bool>,
}

impl WikiManager {
    /// Create a new WikiManager (lightweight, no I/O)
    pub fn new(owner: &str, repo: &str, token: &str) -> Self {
        let cache_dir = Self::get_cache_dir(owner, repo);

        Self {
            owner: owner.to_string(),
            repo: repo.to_string(),
            token: token.to_string(),
            cache_dir,
            initialized: Mutex::new(false),
        }
    }

    /// Create a WikiManager with a specific cache directory, pre-initialized.
    /// For testing only — skips clone/fetch on first use.
    #[cfg(test)]
    pub(crate) fn new_with_cache_dir(owner: &str, repo: &str, token: &str, cache_dir: PathBuf) -> Self {
        Self {
            owner: owner.to_string(),
            repo: repo.to_string(),
            token: token.to_string(),
            cache_dir,
            initialized: Mutex::new(true),
        }
    }

    /// Get cache directory for this wiki
    fn get_cache_dir(owner: &str, repo: &str) -> PathBuf {
        let base_dir = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir());

        base_dir
            .join(".cache")
            .join("track")
            .join("wikis")
            .join(owner)
            .join(repo)
    }

    /// Get wiki repository URL (auth is handled via credentials callback)
    fn wiki_url(&self) -> String {
        format!(
            "https://github.com/{}/{}.wiki.git",
            self.owner, self.repo
        )
    }

    /// Initialize wiki repository (clone or fetch)
    pub fn ensure_initialized(&self) -> Result<()> {
        let mut initialized = self.initialized.lock().map_err(|_| {
            GitHubError::Wiki("Failed to acquire initialization lock".to_string())
        })?;

        if *initialized {
            return Ok(());
        }

        if self.cache_dir.exists() {
            // Repository exists, fetch latest
            self.fetch_and_merge()?;
        } else {
            // Clone wiki repository
            self.clone_wiki()?;
        }

        *initialized = true;
        Ok(())
    }

    /// Clone the wiki repository
    fn clone_wiki(&self) -> Result<()> {
        // Create parent directories
        if let Some(parent) = self.cache_dir.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                GitHubError::Wiki(format!("Failed to create cache directory: {}", e))
            })?;
        }

        let mut callbacks = RemoteCallbacks::new();
        callbacks.credentials(|_url, _username, _allowed| {
            Cred::userpass_plaintext(&self.token, "x-oauth-basic")
        });

        let mut fetch_opts = git2::FetchOptions::new();
        fetch_opts.remote_callbacks(callbacks);

        let mut builder = git2::build::RepoBuilder::new();
        builder.fetch_options(fetch_opts);

        builder
            .clone(&self.wiki_url(), &self.cache_dir)
            .map_err(|e| {
                GitHubError::Wiki(format!(
                    "Failed to clone wiki (wiki may not exist or may be disabled): {}",
                    e
                ))
            })?;

        Ok(())
    }

    /// Fetch and merge latest changes
    fn fetch_and_merge(&self) -> Result<()> {
        let repo = Repository::open(&self.cache_dir).map_err(|e| {
            GitHubError::Wiki(format!("Failed to open wiki repository: {}", e))
        })?;

        let mut remote = repo.find_remote("origin").map_err(|e| {
            GitHubError::Wiki(format!("Failed to find origin remote: {}", e))
        })?;

        let mut callbacks = RemoteCallbacks::new();
        callbacks.credentials(|_url, _username, _allowed| {
            Cred::userpass_plaintext(&self.token, "x-oauth-basic")
        });

        let mut fetch_opts = git2::FetchOptions::new();
        fetch_opts.remote_callbacks(callbacks);

        // GitHub wikis only use the `master` branch
        remote
            .fetch(&["master"], Some(&mut fetch_opts), None)
            .map_err(|e| GitHubError::Wiki(format!("Failed to fetch: {}", e)))?;

        // Merge FETCH_HEAD into current branch
        let fetch_head = repo.find_reference("FETCH_HEAD").map_err(|e| {
            GitHubError::Wiki(format!("Failed to find FETCH_HEAD: {}", e))
        })?;
        let fetch_commit = repo.reference_to_annotated_commit(&fetch_head).map_err(|e| {
            GitHubError::Wiki(format!("Failed to get fetch commit: {}", e))
        })?;

        let analysis = repo.merge_analysis(&[&fetch_commit]).map_err(|e| {
            GitHubError::Wiki(format!("Failed to analyze merge: {}", e))
        })?;

        if analysis.0.is_fast_forward() {
            let mut reference = repo.find_reference("HEAD").map_err(|e| {
                GitHubError::Wiki(format!("Failed to find HEAD: {}", e))
            })?;
            let name = reference
                .name()
                .ok_or_else(|| GitHubError::Wiki("Invalid reference name".to_string()))?
                .to_string();
            let msg = "Fast-forward merge".to_string();
            reference
                .set_target(fetch_commit.id(), &msg)
                .map_err(|e| GitHubError::Wiki(format!("Failed to fast-forward: {}", e)))?;
            repo.set_head(&name)
                .map_err(|e| GitHubError::Wiki(format!("Failed to set HEAD: {}", e)))?;
            repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))
                .map_err(|e| GitHubError::Wiki(format!("Failed to checkout: {}", e)))?;
        } else if analysis.0.is_up_to_date() {
            // Nothing to do
        } else {
            return Err(GitHubError::Wiki(
                "Wiki has diverged; non-fast-forward merge required. \
                 Delete the cache at ~/.cache/track/wikis/ and retry."
                    .to_string(),
            ));
        }

        Ok(())
    }

    /// List all wiki pages
    pub fn list_pages(&self) -> Result<Vec<WikiPage>> {
        self.ensure_initialized()?;

        let mut pages = Vec::new();

        for entry in WalkDir::new(&self.cache_dir)
            .follow_links(false)
            .min_depth(1)
        {
            let entry = entry.map_err(|e| {
                GitHubError::Wiki(format!("Failed to read directory: {}", e))
            })?;

            let path = entry.path();
            
            // Skip special pages, git directory, and non-markdown files
            if self.should_skip_path(path) {
                continue;
            }

            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                if let Ok(page) = self.read_page(path) {
                    pages.push(page);
                }
            }
        }

        Ok(pages)
    }

    /// Check if path should be skipped
    fn should_skip_path(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        
        // Skip .git directory
        if path_str.contains("/.git/") || path_str.ends_with("/.git") {
            return true;
        }

        // Skip special GitHub wiki pages
        if let Some(filename) = path.file_name() {
            let filename_str = filename.to_string_lossy();
            if filename_str.starts_with('_') {
                return true; // _Sidebar.md, _Header.md, _Footer.md
            }
        }

        false
    }

    /// Read a wiki page from disk
    fn read_page(&self, path: &Path) -> Result<WikiPage> {
        let content = fs::read_to_string(path).map_err(|e| {
            GitHubError::Wiki(format!("Failed to read file: {}", e))
        })?;

        // Parse front matter and content
        let (front_matter, content) = self.parse_markdown_with_frontmatter(&content);

        // Get slug from filename
        let relative_path = path
            .strip_prefix(&self.cache_dir)
            .map_err(|_| GitHubError::Wiki("Invalid path".to_string()))?;

        let slug = relative_path
            .with_extension("")
            .to_string_lossy()
            .replace('\\', "/");

        // Extract parent from path
        let parent = relative_path
            .parent()
            .and_then(|p| {
                if p == Path::new("") {
                    None
                } else {
                    Some(p.to_string_lossy().replace('\\', "/"))
                }
            });

        // Get title from front matter or filename
        let title = front_matter
            .title
            .clone()
            .or_else(|| {
                path.file_stem()
                    .map(|s| s.to_string_lossy().replace('-', " "))
            })
            .unwrap_or_else(|| slug.clone());

        // Get timestamps from git
        let (created, updated, author) = self.get_file_timestamps(path)?;

        Ok(WikiPage {
            slug,
            title,
            content,
            parent,
            tags: front_matter.tags,
            created,
            updated,
            author,
        })
    }

    /// Parse markdown with YAML front matter
    fn parse_markdown_with_frontmatter(&self, content: &str) -> (FrontMatter, String) {
        let lines: Vec<&str> = content.lines().collect();
        
        if lines.first() == Some(&"---") {
            // Find closing ---
            if let Some(end_idx) = lines.iter().skip(1).position(|&line| line == "---") {
                let yaml_lines = &lines[1..end_idx + 1];
                let yaml_str = yaml_lines.join("\n");
                
                if let Ok(front_matter) = serde_yaml::from_str::<FrontMatter>(&yaml_str) {
                    let content_start = (end_idx + 2).min(lines.len());
                    let content_lines = &lines[content_start..];
                    let content = content_lines.join("\n");
                    return (front_matter, content);
                }
            }
        }

        (FrontMatter::default(), content.to_string())
    }

    /// Get file timestamps from git history
    fn get_file_timestamps(&self, path: &Path) -> Result<(DateTime<Utc>, DateTime<Utc>, Option<String>)> {
        let repo = Repository::open(&self.cache_dir).map_err(|e| {
            GitHubError::Wiki(format!("Failed to open repository: {}", e))
        })?;

        let relative_path = path
            .strip_prefix(&self.cache_dir)
            .map_err(|_| GitHubError::Wiki("Invalid path".to_string()))?;

        let mut revwalk = repo.revwalk().map_err(|e| {
            GitHubError::Wiki(format!("Failed to walk commits: {}", e))
        })?;
        revwalk.push_head().ok();

        let mut first_commit_time = None;
        let mut last_commit_time = None;
        let mut last_author = None;

        for oid in revwalk.flatten() {
            if let Ok(commit) = repo.find_commit(oid) {
                if let Ok(tree) = commit.tree() {
                    if tree.get_path(relative_path).is_ok() {
                        let time = commit.time();
                        let timestamp = DateTime::from_timestamp(time.seconds(), 0)
                            .unwrap_or_else(Utc::now);

                        if last_commit_time.is_none() {
                            last_commit_time = Some(timestamp);
                            last_author =
                                Some(commit.author().name().unwrap_or("Unknown").to_string());
                        }
                        first_commit_time = Some(timestamp);
                    }
                }
            }
        }

        let created = first_commit_time.unwrap_or_else(Utc::now);
        let updated = last_commit_time.unwrap_or(created);

        Ok((created, updated, last_author))
    }

    /// Get a specific page by slug
    pub fn get_page(&self, slug: &str) -> Result<WikiPage> {
        validate_slug(slug)?;
        self.ensure_initialized()?;

        let page_path = self.cache_dir.join(format!("{}.md", slug));
        
        if !page_path.exists() {
            return Err(GitHubError::Wiki(format!("Page '{}' not found", slug)));
        }

        self.read_page(&page_path)
    }

    /// Create a new wiki page
    pub fn create_page(&self, slug: &str, title: &str, content: &str, tags: Vec<String>) -> Result<WikiPage> {
        validate_slug(slug)?;
        self.ensure_initialized()?;

        let page_path = self.cache_dir.join(format!("{}.md", slug));
        
        if page_path.exists() {
            return Err(GitHubError::Wiki(format!("Page '{}' already exists", slug)));
        }

        // Create parent directories if needed
        if let Some(parent) = page_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                GitHubError::Wiki(format!("Failed to create directory: {}", e))
            })?;
        }

        // Generate markdown with front matter
        let markdown = self.generate_markdown_with_frontmatter(title, content, &tags);
        
        fs::write(&page_path, markdown).map_err(|e| {
            GitHubError::Wiki(format!("Failed to write file: {}", e))
        })?;

        // Commit and push
        self.commit_and_push(&format!("Create {}", slug), &[&page_path])?;

        self.read_page(&page_path)
    }

    /// Update an existing wiki page
    pub fn update_page(&self, slug: &str, title: Option<&str>, content: Option<&str>, tags: Option<Vec<String>>) -> Result<WikiPage> {
        validate_slug(slug)?;
        self.ensure_initialized()?;

        let page_path = self.cache_dir.join(format!("{}.md", slug));
        
        if !page_path.exists() {
            return Err(GitHubError::Wiki(format!("Page '{}' not found", slug)));
        }

        // Read existing page
        let existing = self.read_page(&page_path)?;
        
        let new_title = title.unwrap_or(&existing.title);
        let new_content = content.unwrap_or(&existing.content);
        let new_tags = tags.unwrap_or(existing.tags);

        // Generate updated markdown
        let markdown = self.generate_markdown_with_frontmatter(new_title, new_content, &new_tags);
        
        fs::write(&page_path, markdown).map_err(|e| {
            GitHubError::Wiki(format!("Failed to write file: {}", e))
        })?;

        // Commit and push
        self.commit_and_push(&format!("Update {}", slug), &[&page_path])?;

        self.read_page(&page_path)
    }

    /// Delete a wiki page
    pub fn delete_page(&self, slug: &str) -> Result<()> {
        validate_slug(slug)?;
        self.ensure_initialized()?;

        let page_path = self.cache_dir.join(format!("{}.md", slug));
        
        if !page_path.exists() {
            return Err(GitHubError::Wiki(format!("Page '{}' not found", slug)));
        }

        fs::remove_file(&page_path).map_err(|e| {
            GitHubError::Wiki(format!("Failed to delete file: {}", e))
        })?;

        // Commit and push
        self.commit_and_push(&format!("Delete {}", slug), &[&page_path])?;

        Ok(())
    }

    /// Move a page to a new location
    pub fn move_page(&self, slug: &str, new_parent: Option<&str>) -> Result<WikiPage> {
        validate_slug(slug)?;
        if let Some(parent) = new_parent {
            validate_slug(parent)?;
        }
        self.ensure_initialized()?;

        let old_path = self.cache_dir.join(format!("{}.md", slug));
        
        if !old_path.exists() {
            return Err(GitHubError::Wiki(format!("Page '{}' not found", slug)));
        }

        // Extract filename from slug
        let filename = Path::new(slug)
            .file_name()
            .ok_or_else(|| GitHubError::Wiki("Invalid slug".to_string()))?;

        // Build new path
        let new_path = if let Some(parent) = new_parent {
            self.cache_dir.join(parent).join(filename).with_extension("md")
        } else {
            self.cache_dir.join(filename).with_extension("md")
        };

        // Create parent directory if needed
        if let Some(parent) = new_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                GitHubError::Wiki(format!("Failed to create directory: {}", e))
            })?;
        }

        // Move file
        fs::rename(&old_path, &new_path).map_err(|e| {
            GitHubError::Wiki(format!("Failed to move file: {}", e))
        })?;

        // Commit and push
        self.commit_and_push(
            &format!("Move {} to {}", slug, new_parent.unwrap_or("root")),
            &[&old_path, &new_path],
        )?;

        self.read_page(&new_path)
    }

    /// Generate markdown with YAML front matter
    fn generate_markdown_with_frontmatter(&self, title: &str, content: &str, tags: &[String]) -> String {
        let front_matter = FrontMatter {
            title: Some(title.to_string()),
            tags: tags.to_vec(),
        };

        let yaml = match serde_yaml::to_string(&front_matter) {
            Ok(y) => y.trim_start_matches("---\n").trim().to_string(),
            Err(_) => return format!("{}\n", content),
        };

        format!("---\n{}\n---\n\n{}", yaml, content)
    }

    /// Commit changes and push to remote
    fn commit_and_push(&self, message: &str, paths: &[&Path]) -> Result<()> {
        let repo = Repository::open(&self.cache_dir).map_err(|e| {
            GitHubError::Wiki(format!("Failed to open repository: {}", e))
        })?;

        let mut index = repo.index().map_err(|e| {
            GitHubError::Wiki(format!("Failed to get index: {}", e))
        })?;

        // Add or remove files
        for path in paths {
            let relative_path = path
                .strip_prefix(&self.cache_dir)
                .map_err(|_| GitHubError::Wiki("Invalid path".to_string()))?;

            if path.exists() {
                index.add_path(relative_path).map_err(|e| {
                    GitHubError::Wiki(format!("Failed to add file to index: {}", e))
                })?;
            } else {
                index.remove_path(relative_path).map_err(|e| {
                    GitHubError::Wiki(format!("Failed to remove file from index: {}", e))
                })?;
            }
        }

        index.write().map_err(|e| {
            GitHubError::Wiki(format!("Failed to write index: {}", e))
        })?;

        let tree_id = index.write_tree().map_err(|e| {
            GitHubError::Wiki(format!("Failed to write tree: {}", e))
        })?;

        let tree = repo.find_tree(tree_id).map_err(|e| {
            GitHubError::Wiki(format!("Failed to find tree: {}", e))
        })?;

        let signature = Signature::now("track-cli", "track@example.com").map_err(|e| {
            GitHubError::Wiki(format!("Failed to create signature: {}", e))
        })?;

        let parent_commit = repo.head()
            .and_then(|head| head.peel_to_commit())
            .ok();

        let parents: Vec<&git2::Commit> = parent_commit.iter().collect();

        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &parents,
        )
        .map_err(|e| GitHubError::Wiki(format!("Failed to commit: {}", e)))?;

        // Push to remote
        let mut remote = repo.find_remote("origin").map_err(|e| {
            GitHubError::Wiki(format!("Failed to find remote: {}", e))
        })?;

        let mut callbacks = RemoteCallbacks::new();
        callbacks.credentials(|_url, _username, _allowed| {
            Cred::userpass_plaintext(&self.token, "x-oauth-basic")
        });

        let mut push_opts = git2::PushOptions::new();
        push_opts.remote_callbacks(callbacks);

        // GitHub wikis only support the `master` branch — the web UI
        // ignores all other branches, so always push to master.
        remote
            .push(
                &["refs/heads/master:refs/heads/master"],
                Some(&mut push_opts),
            )
            .map_err(|e| {
                GitHubError::Wiki(format!("Failed to push to master: {}", e))
            })?;

        Ok(())
    }

    /// Search pages by content
    pub fn search_pages(&self, query: &str) -> Result<Vec<WikiPage>> {
        let pages = self.list_pages()?;
        let query_lower = query.to_lowercase();

        let filtered: Vec<WikiPage> = pages
            .into_iter()
            .filter(|page| {
                page.title.to_lowercase().contains(&query_lower)
                    || page.content.to_lowercase().contains(&query_lower)
                    || page.tags.iter().any(|t| t.to_lowercase().contains(&query_lower))
            })
            .collect();

        Ok(filtered)
    }

    /// Get child pages of a parent directory
    pub fn get_child_pages(&self, parent_slug: &str) -> Result<Vec<WikiPage>> {
        let pages = self.list_pages()?;

        let children: Vec<WikiPage> = pages
            .into_iter()
            .filter(|page| {
                page.parent.as_deref() == Some(parent_slug)
            })
            .collect();

        Ok(children)
    }
}
