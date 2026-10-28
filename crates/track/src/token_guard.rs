//! Keeps a repository's `./.track.toml` from sending your token to a host it
//! chose.
//!
//! Config layers merge key by key, so a committed `.track.toml` that sets only
//! a URL would otherwise send the token from your global config, environment
//! or `--token` to that URL. [`find_token_redirect`] detects exactly that case;
//! [`ensure_token_destination_allowed`] refuses it unless the user trusted the
//! destination with `track config trust`.

use crate::cli::Backend;
use crate::config::{self, Config};
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// File in the global config directory recording trusted destinations.
const TRUST_STORE_FILE: &str = "trusted-configs.toml";

/// A repository config sending a token it did not supply to a host it chose.
#[derive(Debug, Serialize)]
pub struct TokenRedirect {
    pub backend: Backend,
    /// Canonical path of the repository's `.track.toml`.
    pub project_config: PathBuf,
    /// Origin the token would be sent to, set by `project_config`.
    pub endpoint: String,
    /// Origin your own configuration sends the token to, if it sets one.
    pub own_endpoint: Option<String>,
    /// Whether `track config trust` allowed this destination.
    pub trusted: bool,
}

impl TokenRedirect {
    fn refusal(&self) -> anyhow::Error {
        let own = match &self.own_endpoint {
            Some(own) => format!("your own configuration points it at {own}"),
            None => "your own configuration sets no URL for it".to_string(),
        };
        // Pointing --config at the global file only helps if there is one.
        let ignore_repository_config = config::global_config_path()
            .filter(|path| path.exists())
            .map(|path| {
                format!(
                    "\n  - To ignore this repository's config: track --config {} ...",
                    path.display()
                )
            })
            .unwrap_or_default();
        anyhow!(
            "Refusing to send your {backend} token to {endpoint}.\n\
             {config} sets that URL but not the token, and {own}. A repository's \
             config could use this to collect your credentials.\n  \
             - If you trust this repository: track config trust\
             {ignore_repository_config}\n  \
             - Or pass the URL explicitly with --url",
            backend = self.backend,
            endpoint = self.endpoint,
            config = self.project_config.display(),
        )
    }

    fn trust_entry(&self) -> TrustedDestination {
        TrustedDestination {
            config: self.project_config.clone(),
            backend: self.backend,
            endpoint: self.endpoint.clone(),
        }
    }
}

/// Detect the current directory's `.track.toml` redirecting a token it did
/// not supply.
///
/// `effective` is the config every command uses: loaded for `backend` and
/// merged with the `--url`/`--token` flags. It is compared with the same
/// config built without the repository file. A redirect is a token both
/// agree on (so it is yours) sent to an origin only the repository file sets.
pub fn find_token_redirect(
    explicit_config: Option<&Path>,
    backend: Backend,
    cli_url: Option<&str>,
    cli_token: Option<&str>,
    effective: &Config,
) -> Result<Option<TokenRedirect>> {
    // An explicit --config file replaces the repository file entirely.
    if explicit_config.is_some() {
        return Ok(None);
    }
    let Some(project_config) = repository_config_path()? else {
        return Ok(None);
    };
    let Some(token) = effective.token.as_deref() else {
        return Ok(None);
    };

    let mut own = Config::load_without_project_file(backend)?;
    own.merge_with_cli(cli_url.map(String::from), cli_token.map(String::from));
    if own.token.as_deref() != Some(token) {
        // The repository file supplies its own token: nothing of yours leaks.
        return Ok(None);
    }

    let endpoint = effective.token_endpoint(backend).map(origin);
    let own_endpoint = own.token_endpoint(backend).map(origin);
    let Some(endpoint) = endpoint.filter(|endpoint| Some(endpoint) != own_endpoint.as_ref()) else {
        return Ok(None);
    };

    let mut redirect = TokenRedirect {
        backend,
        project_config,
        endpoint,
        own_endpoint,
        trusted: false,
    };
    redirect.trusted = load_trust_store()?
        .trusted
        .contains(&redirect.trust_entry());
    Ok(Some(redirect))
}

/// Fail before any request if the repository config redirects your token to
/// a destination that was not trusted with `track config trust`.
pub fn ensure_token_destination_allowed(
    explicit_config: Option<&Path>,
    backend: Backend,
    cli_url: Option<&str>,
    cli_token: Option<&str>,
    effective: &Config,
) -> Result<()> {
    match find_token_redirect(explicit_config, backend, cli_url, cli_token, effective)? {
        Some(redirect) if !redirect.trusted => Err(redirect.refusal()),
        _ => Ok(()),
    }
}

/// Record `redirect` as trusted. Returns false if it already was.
pub fn trust(redirect: &TokenRedirect) -> Result<bool> {
    let mut store = load_trust_store()?;
    let entry = redirect.trust_entry();
    if store.trusted.contains(&entry) {
        return Ok(false);
    }
    store.trusted.push(entry);
    save_trust_store(&store)?;
    Ok(true)
}

/// Forget every trusted destination of the current directory's `.track.toml`.
/// Returns how many were removed.
pub fn untrust_repository_config() -> Result<usize> {
    let Some(project_config) = repository_config_path()? else {
        return Ok(0);
    };
    let mut store = load_trust_store()?;
    let before = store.trusted.len();
    store.trusted.retain(|entry| entry.config != project_config);
    let removed = before - store.trusted.len();
    if removed > 0 {
        save_trust_store(&store)?;
    }
    Ok(removed)
}

/// Canonical path of the current directory's `.track.toml`, if it exists and
/// is not the global config itself (running from `~/.tracker-cli`).
fn repository_config_path() -> Result<Option<PathBuf>> {
    let path = config::local_track_config_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let path = fs::canonicalize(&path)?;
    let is_global = config::global_config_path()
        .and_then(|global| fs::canonicalize(global).ok())
        .is_some_and(|global| global == path);
    Ok((!is_global).then_some(path))
}

/// Scheme, host and port of `url`, lowercased and without the scheme's
/// default port, so equivalent spellings compare equal. An unparsable URL is
/// kept as written, which only ever makes two URLs compare unequal.
fn origin(url: &str) -> String {
    let Ok(uri) = url.trim().parse::<ureq::http::Uri>() else {
        return url.trim().to_string();
    };
    let (Some(scheme), Some(host)) = (uri.scheme_str(), uri.host()) else {
        return url.trim().to_string();
    };
    let scheme = scheme.to_ascii_lowercase();
    let host = host.to_ascii_lowercase();
    let default_port = match scheme.as_str() {
        "https" => Some(443),
        "http" => Some(80),
        _ => None,
    };
    match uri.port_u16().filter(|port| Some(*port) != default_port) {
        Some(port) => format!("{scheme}://{host}:{port}"),
        None => format!("{scheme}://{host}"),
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct TrustStore {
    #[serde(default)]
    trusted: Vec<TrustedDestination>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct TrustedDestination {
    config: PathBuf,
    backend: Backend,
    endpoint: String,
}

fn trust_store_path() -> Option<PathBuf> {
    config::global_config_path().map(|path| path.with_file_name(TRUST_STORE_FILE))
}

fn load_trust_store() -> Result<TrustStore> {
    let Some(path) = trust_store_path().filter(|path| path.exists()) else {
        return Ok(TrustStore::default());
    };
    let contents = fs::read_to_string(&path)?;
    toml::from_str(&contents).map_err(|e| anyhow!("Failed to read {}: {}", path.display(), e))
}

fn save_trust_store(store: &TrustStore) -> Result<()> {
    // Ensures ~/.tracker-cli exists with private permissions.
    config::global_config_path_ensure()?;
    let path = trust_store_path()
        .ok_or_else(|| anyhow!("Could not determine home directory for {TRUST_STORE_FILE}"))?;
    let contents = format!(
        "# Repository configs allowed to send a token from your own configuration\n\
         # to a URL they set. Managed by `track config trust` / `track config untrust`.\n\n{}",
        toml::to_string_pretty(store)?
    );
    fs::write(&path, contents)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_ignores_case_paths_and_default_ports() {
        assert_eq!(
            origin("HTTPS://Acme.YouTrack.cloud/youtrack/"),
            "https://acme.youtrack.cloud"
        );
        assert_eq!(
            origin("https://acme.example:443/api"),
            "https://acme.example"
        );
        assert_eq!(origin("http://127.0.0.1:8080"), "http://127.0.0.1:8080");
    }

    #[test]
    fn origin_distinguishes_scheme_port_and_userinfo_tricks() {
        assert_ne!(
            origin("http://acme.example"),
            origin("https://acme.example")
        );
        assert_ne!(
            origin("https://acme.example:8443"),
            origin("https://acme.example")
        );
        // The host is what follows the userinfo.
        assert_eq!(
            origin("https://acme.example@evil.example/"),
            "https://evil.example"
        );
    }
}
