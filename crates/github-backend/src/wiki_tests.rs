//! Unit tests for GitHub WikiManager

#[cfg(test)]
mod tests {
    use crate::WikiManager;
    use git2::{Repository, Signature};
    use std::env;
    use std::fs;
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;

    struct EnvGuard {
        key: String,
        previous: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &str, value: &str) -> Self {
            let previous = env::var(key).ok();
            env::set_var(key, value);
            Self {
                key: key.to_string(),
                previous,
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(value) = &self.previous {
                env::set_var(&self.key, value);
            } else {
                env::remove_var(&self.key);
            }
        }
    }

    fn commit_all(repo: &Repository, message: &str) -> git2::Oid {
        let mut index = repo.index().expect("index");
        index.add_all(["*"] , git2::IndexAddOption::DEFAULT, None).expect("add");
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

    fn setup_bare_repo(root: &Path) -> (PathBuf, PathBuf) {
        let bare_path = root.join("bare.git");
        let work_path = root.join("work");

        let _bare = Repository::init_bare(&bare_path).expect("init bare");
        let work = Repository::init(&work_path).expect("init work");

        write_file(
            &work_path.join("Home.md"),
            "# Home\nWelcome",
        );
        write_file(
            &work_path.join("_Sidebar.md"),
            "# Sidebar",
        );
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

        (bare_path, work_path)
    }

    #[test]
    fn wiki_manager_crud_and_listing() {
        let temp = TempDir::new().expect("tempdir");
        let _home = EnvGuard::set("HOME", temp.path().to_str().expect("home"));

        let (bare_path, _work_path) = setup_bare_repo(temp.path());

        let cache_dir = temp
            .path()
            .join(".cache")
            .join("track")
            .join("wikis")
            .join("owner")
            .join("repo");

        Repository::clone(bare_path.to_str().expect("bare path"), &cache_dir)
            .expect("clone to cache");

        let wiki = WikiManager::new("owner", "repo", "token").expect("wiki manager");

        let pages = wiki.list_pages().expect("list pages");
        assert!(pages.iter().any(|p| p.slug == "Home"));
        assert!(!pages.iter().any(|p| p.slug.contains("_Sidebar")));
        assert!(pages.iter().any(|p| p.slug == "Tutorials/Getting-Started"));

        let page = wiki.get_page("Tutorials/Getting-Started").expect("get page");
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
}
