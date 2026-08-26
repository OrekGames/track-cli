//! Failing pin: README Quick Start must include the GitHub first-time path (#320).
//!
//! Reads the repo-root README via `CARGO_MANIFEST_DIR` (this crate is
//! `crates/track`, so the README is two levels up). These assertions match the
//! #356 design: copy-pasteable `track init --backend github` plus
//! `track i s "is:open"`. They fail on current main, where Quick Start only
//! shows YouTrack / Jira / Linear.

use std::path::PathBuf;

const GITHUB_INIT: &str = "track init --backend github --url https://api.github.com --token YOUR_GITHUB_TOKEN --project owner/repo";
const GITHUB_FIRST_SEARCH: &str = "track i s \"is:open\"";

fn readme_markdown() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../README.md");
    std::fs::read_to_string(&path).unwrap_or_else(|err| {
        panic!(
            "failed to read repo-root README.md via CARGO_MANIFEST_DIR at {}: {err}",
            path.display()
        )
    })
}

fn heading_level_and_title(line: &str) -> Option<(usize, &str)> {
    let line = line.trim_end_matches('\r');
    let level = line.chars().take_while(|c| *c == '#').count();
    if !(1..=6).contains(&level) {
        return None;
    }
    let rest = &line[level..];
    if !rest.starts_with(' ') {
        return None;
    }
    Some((level, rest.trim()))
}

fn section_body<'a>(markdown: &'a str, title: &str) -> Option<&'a str> {
    let mut start = None;
    let mut level = 0;
    let mut offset = 0;
    for line in markdown.split_inclusive('\n') {
        let content = line.trim_end_matches(['\n', '\r']);
        if let Some((lvl, heading)) = heading_level_and_title(content) {
            if start.is_none() && heading == title {
                start = Some(offset + line.len());
                level = lvl;
            } else if start.is_some() && lvl <= level {
                return Some(markdown[start.unwrap()..offset].trim());
            }
        }
        offset += line.len();
    }
    start.map(|s| markdown[s..].trim())
}

fn quick_start_init_section() -> String {
    let readme = readme_markdown();
    let quick_start = section_body(&readme, "Quick Start")
        .expect("README.md must contain a ## Quick Start section");
    section_body(quick_start, "1. Initialize Configuration")
        .expect("Quick Start must contain ### 1. Initialize Configuration")
        .to_string()
}

/// Treat the spec's backslash-wrapped command as equal to the one-liner form.
fn unwrap_shell_continuations(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' {
            let mut j = i + 1;
            if j < chars.len() && chars[j] == '\r' {
                j += 1;
            }
            if j < chars.len() && chars[j] == '\n' {
                j += 1;
                while j < chars.len() && (chars[j] == ' ' || chars[j] == '\t') {
                    j += 1;
                }
                out.push(' ');
                i = j;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn collapse_ws(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn find_command(haystack: &str, command: &str) -> Option<usize> {
    collapse_ws(&unwrap_shell_continuations(haystack)).find(&collapse_ws(command))
}

fn github_init_commands(section: &str) -> Vec<String> {
    unwrap_shell_continuations(section)
        .lines()
        .map(collapse_ws)
        .filter(|line| {
            let tokens: Vec<&str> = line.split_whitespace().collect();
            tokens.windows(2).any(|pair| pair == ["track", "init"])
                && tokens
                    .windows(2)
                    .any(|pair| pair == ["--backend", "github"])
        })
        .collect()
}

fn flag_value<'a>(command: &'a str, flag: &str) -> Option<&'a str> {
    let mut tokens = command.split_whitespace();
    while let Some(tok) = tokens.next() {
        if tok == flag {
            return tokens.next();
        }
    }
    None
}

fn has_flag_token(command: &str, flag: &str) -> bool {
    command.split_whitespace().any(|tok| tok == flag)
}

#[test]
fn readme_quick_start_includes_github_init_and_search() {
    let section = quick_start_init_section();
    let init_at = find_command(&section, GITHUB_INIT).unwrap_or_else(|| {
        panic!(
            "README Quick Start / Initialize Configuration is missing the GitHub first-time path; \
             expected command {GITHUB_INIT:?}"
        )
    });
    let search_at = find_command(&section, GITHUB_FIRST_SEARCH).unwrap_or_else(|| {
        panic!(
            "README Quick Start / Initialize Configuration is missing the GitHub first-search; \
             expected {GITHUB_FIRST_SEARCH:?}"
        )
    });
    assert!(
        init_at < search_at,
        "GitHub init must appear before {GITHUB_FIRST_SEARCH:?} in Quick Start / Initialize Configuration"
    );
}

#[test]
fn readme_github_quick_start_init_uses_api_url_and_project() {
    let section = quick_start_init_section();
    let commands = github_init_commands(&section);
    assert!(
        !commands.is_empty(),
        "README Quick Start / Initialize Configuration is missing the GitHub first-time path \
         (`track init --backend github`)"
    );

    for command in &commands {
        assert!(
            command.contains("--backend github"),
            "GitHub Quick Start init must include `--backend github`: {command}"
        );

        let url = flag_value(command, "--url")
            .unwrap_or_else(|| panic!("GitHub Quick Start init is missing `--url`: {command}"));
        assert!(
            url.contains("api.github.com"),
            "GitHub Quick Start init `--url` must contain api.github.com, got {url:?}"
        );
        assert_ne!(
            url.trim_end_matches('/'),
            "https://github.com",
            "GitHub Quick Start init `--url` must not be the web URL https://github.com: {command}"
        );

        assert!(
            command.contains("--project owner/repo"),
            "GitHub Quick Start init must use `--project owner/repo`, not `--owner` / `--repo`: {command}"
        );
        assert!(
            !has_flag_token(command, "--owner") && !has_flag_token(command, "--repo"),
            "GitHub Quick Start init must not use `--owner` / `--repo` flags: {command}"
        );
    }
}
