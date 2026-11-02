//! Unit tests for GitHub WikiManager

#[cfg(test)]
mod tests {
    use crate::wiki::{WikiManager, validate_slug};
    use git2::{Repository, Signature};
    use std::fs;
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;
    use tracker_core::{AttachmentUpload, AttachmentUploadFile, TrackerError};

    fn commit_all(repo: &Repository, message: &str) -> git2::Oid {
        let mut index = repo.index().expect("index");
        index
            .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
            .expect("add");
        index.write().expect("index write");
        let tree_id = index.write_tree().expect("write tree");
        let tree = repo.find_tree(tree_id).expect("find tree");
        let signature = Signature::now("tester", "tester@example.com").expect("signature");

        let parent = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
        let parents = parent.iter().collect::<Vec<_>>();

        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &parents,
        )
        .expect("commit")
    }

    fn write_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("mkdir");
        }
        fs::write(path, content).expect("write");
    }

    /// Set up a bare repo and clone it to a cache dir, returning
    /// a pre-initialized WikiManager that doesn't need network access.
    fn setup_wiki(temp: &TempDir) -> (WikiManager, PathBuf) {
        let bare_path = temp.path().join("bare.git");
        let work_path = temp.path().join("work");

        let _bare = Repository::init_bare(&bare_path).expect("init bare");
        let work = Repository::init(&work_path).expect("init work");

        write_file(&work_path.join("Home.md"), "# Home\nWelcome");
        write_file(&work_path.join("_Sidebar.md"), "# Sidebar");
        write_file(
            &work_path.join("Tutorials/Getting-Started.md"),
            "---\ntitle: Getting Started\ntags:\n  - intro\n  - setup\n---\n\n# Hello",
        );

        let first_commit = commit_all(&work, "Initial wiki");

        if work.find_reference("refs/heads/main").is_err() {
            work.branch(
                "main",
                &work.find_commit(first_commit).expect("find commit"),
                false,
            )
            .expect("branch main");
        }

        work.remote("origin", bare_path.to_str().expect("bare path"))
            .expect("remote");

        let mut remote = work.find_remote("origin").expect("find remote");
        let mut refspecs = Vec::new();
        if work.find_reference("refs/heads/master").is_ok() {
            refspecs.push("refs/heads/master:refs/heads/master");
        }
        if work.find_reference("refs/heads/main").is_ok() {
            refspecs.push("refs/heads/main:refs/heads/main");
        }
        remote.push(&refspecs, None).expect("push");

        // Clone from bare repo to the cache directory
        let cache_dir = temp.path().join("cache");
        Repository::clone(bare_path.to_str().expect("bare path"), &cache_dir)
            .expect("clone to cache");

        // Create WikiManager pointing at the cache dir, pre-initialized
        let wiki = WikiManager::new_with_cache_dir("owner", "repo", "token", cache_dir.clone());

        (wiki, cache_dir)
    }

    #[test]
    fn wiki_manager_crud_and_listing() {
        let temp = TempDir::new().expect("tempdir");
        let (wiki, _cache_dir) = setup_wiki(&temp);

        let pages = wiki.list_pages().expect("list pages");
        assert!(pages.iter().any(|p| p.slug == "Home"));
        assert!(!pages.iter().any(|p| p.slug.contains("_Sidebar")));
        assert!(pages.iter().any(|p| p.slug == "Tutorials/Getting-Started"));

        let page = wiki
            .get_page("Tutorials/Getting-Started")
            .expect("get page");
        assert_eq!(page.title, "Getting Started");
        assert_eq!(page.tags, vec!["intro".to_string(), "setup".to_string()]);
        assert_eq!(page.parent, Some("Tutorials".to_string()));

        let created = wiki
            .create_page("New-Page", "New Page", "Body", vec!["tag1".to_string()])
            .expect("create page");
        assert_eq!(created.title, "New Page");

        let updated = wiki
            .update_page(
                "New-Page",
                Some("New Title"),
                Some("Updated"),
                Some(vec!["tag2".to_string()]),
            )
            .expect("update page");
        assert_eq!(updated.title, "New Title");
        assert_eq!(updated.tags, vec!["tag2".to_string()]);

        let moved = wiki
            .move_page("New-Page", Some("Guides"))
            .expect("move page");
        assert_eq!(moved.slug, "Guides/New-Page");
        assert_eq!(moved.parent, Some("Guides".to_string()));

        wiki.delete_page("Guides/New-Page").expect("delete page");
        assert!(wiki.get_page("Guides/New-Page").is_err());
    }

    // ====================================================================
    // Security: Path traversal and slug validation
    // ====================================================================

    #[test]
    fn validate_slug_rejects_path_traversal() {
        assert!(validate_slug("../../../etc/passwd").is_err());
        assert!(validate_slug("foo/../bar").is_err());
        assert!(validate_slug("..").is_err());
    }

    #[test]
    fn validate_slug_rejects_absolute_paths() {
        assert!(validate_slug("/etc/passwd").is_err());
        assert!(validate_slug("\\windows\\system32").is_err());
    }

    #[test]
    fn validate_slug_rejects_empty_and_null() {
        assert!(validate_slug("").is_err());
        assert!(validate_slug("foo\0bar").is_err());
    }

    #[test]
    fn validate_slug_accepts_valid_slugs() {
        assert!(validate_slug("Home").is_ok());
        assert!(validate_slug("Tutorials/Getting-Started").is_ok());
        assert!(validate_slug("my-page").is_ok());
    }

    #[test]
    fn get_page_rejects_path_traversal() {
        let temp = TempDir::new().expect("tempdir");
        let (wiki, _) = setup_wiki(&temp);

        let result = wiki.get_page("../../../etc/passwd");
        assert!(result.is_err());
    }

    // ====================================================================
    // Search
    // ====================================================================

    #[test]
    fn search_pages_finds_content() {
        let temp = TempDir::new().expect("tempdir");
        let (wiki, _) = setup_wiki(&temp);

        let results = wiki.search_pages("Welcome").expect("search");
        assert!(results.iter().any(|p| p.slug == "Home"));
    }

    #[test]
    fn search_pages_finds_by_title() {
        let temp = TempDir::new().expect("tempdir");
        let (wiki, _) = setup_wiki(&temp);

        let results = wiki.search_pages("Getting Started").expect("search");
        assert!(
            results
                .iter()
                .any(|p| p.slug == "Tutorials/Getting-Started")
        );
    }

    #[test]
    fn search_pages_finds_by_tag() {
        let temp = TempDir::new().expect("tempdir");
        let (wiki, _) = setup_wiki(&temp);

        let results = wiki.search_pages("intro").expect("search");
        assert!(
            results
                .iter()
                .any(|p| p.slug == "Tutorials/Getting-Started")
        );
    }

    // ====================================================================
    // Frontmatter edge cases
    // ====================================================================

    #[test]
    fn parse_empty_frontmatter() {
        let temp = TempDir::new().expect("tempdir");
        let (wiki, cache_dir) = setup_wiki(&temp);

        // Write a file with empty frontmatter directly to avoid push
        let page_path = cache_dir.join("EmptyFm.md");
        fs::write(
            &page_path,
            "---\ntitle: Empty FM\ntags: []\n---\n\nSome content",
        )
        .expect("write");

        // Commit locally
        let repo = Repository::open(&cache_dir).expect("open repo");
        let mut index = repo.index().expect("index");
        index.add_path(Path::new("EmptyFm.md")).expect("add path");
        index.write().expect("write index");
        let tree_id = index.write_tree().expect("write tree");
        let tree = repo.find_tree(tree_id).expect("find tree");
        let sig = Signature::now("test", "test@example.com").expect("sig");
        let parent = repo.head().and_then(|h| h.peel_to_commit()).expect("head");
        repo.commit(Some("HEAD"), &sig, &sig, "add emptyfm", &tree, &[&parent])
            .expect("commit");

        let page = wiki.get_page("EmptyFm").expect("get page");
        assert_eq!(page.title, "Empty FM");
    }

    #[test]
    fn parse_frontmatter_as_last_content() {
        let temp = TempDir::new().expect("tempdir");
        let (wiki, cache_dir) = setup_wiki(&temp);

        // Write a file that has frontmatter but no body after it
        let fmonly_path = cache_dir.join("FmOnly.md");
        fs::write(&fmonly_path, "---\ntitle: Only FM\n---").expect("write");

        // Commit locally
        let repo = Repository::open(&cache_dir).expect("open repo");
        let mut index = repo.index().expect("index");
        index.add_path(Path::new("FmOnly.md")).expect("add path");
        index.write().expect("write index");
        let tree_id = index.write_tree().expect("write tree");
        let tree = repo.find_tree(tree_id).expect("find tree");
        let sig = Signature::now("test", "test@example.com").expect("sig");
        let parent = repo.head().and_then(|h| h.peel_to_commit()).expect("head");
        repo.commit(Some("HEAD"), &sig, &sig, "add fmonly", &tree, &[&parent])
            .expect("commit");

        // This should NOT panic (C3 fix)
        let page = wiki.get_page("FmOnly").expect("get page");
        assert_eq!(page.title, "Only FM");
        assert!(page.content.is_empty());
    }

    // ====================================================================
    // Error cases
    // ====================================================================

    #[test]
    fn get_nonexistent_page_returns_error() {
        let temp = TempDir::new().expect("tempdir");
        let (wiki, _) = setup_wiki(&temp);

        assert!(wiki.get_page("nonexistent").is_err());
    }

    #[test]
    fn delete_nonexistent_page_returns_error() {
        let temp = TempDir::new().expect("tempdir");
        let (wiki, _) = setup_wiki(&temp);

        assert!(wiki.delete_page("nonexistent").is_err());
    }

    /// Unwrap a `TrackerError::Api` into its status and message.
    fn api_error(err: TrackerError) -> (u16, String) {
        match err {
            TrackerError::Api { status, message } => (status, message),
            other => panic!("Expected Api error, got: {:?}", other),
        }
    }

    #[test]
    fn wiki_errors_map_to_api_500() {
        let temp = TempDir::new().expect("tempdir");
        let (wiki, _) = setup_wiki(&temp);

        // The KnowledgeBase methods pass this error through unchanged.
        let err = wiki.get_page("nonexistent").unwrap_err();

        // A missing page is a 500, not NotFound.
        let (status, message) = api_error(err);
        assert_eq!(status, 500);
        assert!(message.starts_with("Wiki error: "), "{message}");
        assert!(message.contains("not found"), "{message}");
    }

    #[test]
    fn wiki_input_errors_also_map_to_api_500() {
        let temp = TempDir::new().expect("tempdir");
        let (wiki, _) = setup_wiki(&temp);
        let upload = AttachmentUpload {
            files: Vec::new(),
            comment: Some("note".to_string()),
            silent: false,
            minor_edit: true,
        };

        // None of these become InvalidInput.
        let failures = [
            (wiki.get_page("../escape").map(|_| ()), "Invalid slug"),
            (
                wiki.create_page("Home", "Home", "Body", Vec::new())
                    .map(|_| ()),
                "already exists",
            ),
            (
                wiki.add_attachments("Home", &upload).map(|_| ()),
                "does not support --comment",
            ),
        ];
        for (result, expected) in failures {
            let (status, message) = api_error(result.unwrap_err());
            assert_eq!(status, 500, "{message}");
            assert!(message.starts_with("Wiki error: "), "{message}");
            assert!(message.contains(expected), "{message}");
        }
    }

    // ====================================================================
    // Special page filtering
    // ====================================================================

    #[test]
    fn special_pages_filtered_from_listing() {
        let temp = TempDir::new().expect("tempdir");
        let (wiki, _) = setup_wiki(&temp);

        let pages = wiki.list_pages().expect("list");
        for page in &pages {
            assert!(
                !page.slug.starts_with('_'),
                "Special page {} should be filtered",
                page.slug
            );
        }
    }

    #[test]
    fn list_pages_skips_ignored_files() {
        let temp = TempDir::new().expect("tempdir");
        let (wiki, cache_dir) = setup_wiki(&temp);

        // Add non-markdown file
        let txt_path = cache_dir.join("document.txt");
        fs::write(&txt_path, "Some text content").expect("write");

        // Add file in .track-attachments
        let attachment_dir = cache_dir.join(".track-attachments");
        fs::create_dir_all(&attachment_dir).expect("mkdir");
        let attachment_path = attachment_dir.join("attachment.md");
        fs::write(&attachment_path, "Attachment content").expect("write");

        // Add file in .git (simulated, since git operations might overwrite,
        // we just create a file in the existing .git dir)
        let git_dir = cache_dir.join(".git").join("custom_dir");
        fs::create_dir_all(&git_dir).expect("mkdir");
        let git_file_path = git_dir.join("git_internal.md");
        fs::write(&git_file_path, "Git internal content").expect("write");

        let pages = wiki.list_pages().expect("list pages");

        let mut slugs: Vec<String> = pages.into_iter().map(|p| p.slug).collect();
        slugs.sort();

        assert_eq!(slugs, vec!["Home", "Tutorials/Getting-Started"]);
    }

    #[test]
    fn should_skip_path_normalizes_windows_paths() {
        let temp = TempDir::new().expect("tempdir");
        let (wiki, _) = setup_wiki(&temp);

        let p1 = Path::new(r"C:\foo\.git\bar.md");
        assert!(wiki.should_skip_path(p1));

        let p2 = Path::new(r"C:\foo\.track-attachments\img.png");
        assert!(wiki.should_skip_path(p2));

        let p3 = Path::new(r"C:\foo\normal_page.md");
        assert!(!wiki.should_skip_path(p3));
    }

    // ====================================================================
    // has_children via parent field
    // ====================================================================

    #[test]
    fn child_pages_detected() {
        let temp = TempDir::new().expect("tempdir");
        let (wiki, _) = setup_wiki(&temp);

        let children = wiki.get_child_pages("Tutorials").expect("children");
        assert!(!children.is_empty());
        assert!(
            children
                .iter()
                .any(|p| p.slug == "Tutorials/Getting-Started")
        );
    }

    #[test]
    fn no_children_for_leaf_page() {
        let temp = TempDir::new().expect("tempdir");
        let (wiki, _) = setup_wiki(&temp);

        let children = wiki.get_child_pages("Home").expect("children");
        assert!(children.is_empty());
    }

    // ====================================================================
    // Round trips and git history
    // ====================================================================

    #[test]
    fn rewriting_a_page_with_front_matter_keeps_its_content() {
        let temp = TempDir::new().expect("tempdir");
        let (wiki, _) = setup_wiki(&temp);

        let created = wiki
            .create_page("Tagged", "Tagged", "Body", vec!["tag".to_string()])
            .expect("create page");
        assert_eq!(created.content, "Body");

        // Each rewrite re-reads the stored content; the blank line separating
        // front matter from the body must not leak into it.
        wiki.update_page("Tagged", Some("Renamed"), None, None)
            .expect("first update");
        let updated = wiki
            .update_page("Tagged", Some("Renamed again"), None, None)
            .expect("second update");
        assert_eq!(updated.content, "Body");
    }

    #[test]
    fn page_history_reflects_commits_that_touched_the_page() {
        let temp = TempDir::new().expect("tempdir");
        let (wiki, _) = setup_wiki(&temp);

        // A later commit by another author that does not touch Home.
        wiki.create_page("Other", "Other", "Body", Vec::new())
            .expect("create page");

        let home = wiki.get_page("Home").expect("get page");
        assert_eq!(home.author.as_deref(), Some("tester"));
        let other = wiki.get_page("Other").expect("get page");
        assert_eq!(other.author.as_deref(), Some("track-cli"));
    }

    #[test]
    fn move_page_keeps_dots_in_the_page_name() {
        let temp = TempDir::new().expect("tempdir");
        let (wiki, cache_dir) = setup_wiki(&temp);
        wiki.create_page("Release-1.2", "Release 1.2", "Notes", Vec::new())
            .expect("create page");

        let moved = wiki
            .move_page("Release-1.2", Some("Archive"))
            .expect("move page");

        assert_eq!(moved.slug, "Archive/Release-1.2");
        assert!(cache_dir.join("Archive/Release-1.2.md").exists());
    }

    #[test]
    fn move_page_refuses_to_overwrite_an_existing_page() {
        let temp = TempDir::new().expect("tempdir");
        let (wiki, _) = setup_wiki(&temp);
        wiki.create_page("Guides/Home", "Home", "Guides home", Vec::new())
            .expect("create page");

        assert!(wiki.move_page("Home", Some("Guides")).is_err());

        let existing = wiki.get_page("Guides/Home").expect("destination kept");
        assert_eq!(existing.content, "Guides home");
        assert!(wiki.get_page("Home").is_ok(), "source should stay in place");
    }

    #[test]
    fn rejected_push_is_an_error() {
        let temp = TempDir::new().expect("tempdir");
        let (wiki, _) = setup_wiki(&temp);
        // A held ref lock makes the remote reject the update to master.
        fs::write(temp.path().join("bare.git/refs/heads/master.lock"), "")
            .expect("lock remote ref");

        let result = wiki.create_page("Unpublished", "Unpublished", "Body", Vec::new());

        let err = result.expect_err("push rejection should fail the write");
        assert!(err.to_string().contains("rejected"), "got: {err}");
    }

    #[test]
    fn wiki_url_uses_the_web_host_of_the_api() {
        let cases = [
            ("https://api.github.com", "https://github.com/o/r.wiki.git"),
            (
                "https://github.example.com/api/v3",
                "https://github.example.com/o/r.wiki.git",
            ),
            (
                "https://api.octocorp.ghe.com",
                "https://octocorp.ghe.com/o/r.wiki.git",
            ),
        ];
        for (api_url, wiki_url) in cases {
            let wiki = WikiManager::new(api_url, "o", "r", "token");
            assert_eq!(wiki.wiki_url(), wiki_url, "API URL {api_url}");
        }
    }

    #[test]
    fn managed_attachments_are_added_and_listed() {
        let temp = TempDir::new().expect("tempdir");
        let (wiki, cache_dir) = setup_wiki(&temp);
        let source = temp.path().join("diagram.png");
        fs::write(&source, b"png body").expect("write attachment");

        let upload = AttachmentUpload {
            files: vec![AttachmentUploadFile {
                path: source,
                name: Some("diagram.png".to_string()),
                mime_type: Some("image/png".to_string()),
            }],
            comment: None,
            silent: false,
            minor_edit: true,
        };

        let uploaded = wiki.add_attachments("Home", &upload).expect("upload");
        assert_eq!(uploaded.len(), 1);
        assert_eq!(uploaded[0].name, "diagram.png");
        assert_eq!(uploaded[0].mime_type.as_deref(), Some("image/png"));

        let copied = cache_dir.join(".track-attachments/Home/diagram.png");
        assert!(copied.exists(), "attachment should be copied into wiki");

        let page = wiki.get_page("Home").expect("get page");
        assert!(page.content.contains("track:attachments:start"));
        assert!(
            page.content
                .contains("[diagram.png](.track-attachments/Home/diagram.png)")
        );

        let listed = wiki.list_attachments("Home").expect("list attachments");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "diagram.png");
    }
}
