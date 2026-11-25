//! Child-process plumbing shared by the runners that drive an agent CLI.

use anyhow::{Result, bail};
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, ExitStatus};
use std::thread::JoinHandle;

/// How much of a failed CLI's stderr to quote in the error, counted from the end
/// where the cause usually is.
const STDERR_TAIL_BYTES: usize = 2000;

/// The environment of a scored `track` command, wherever it is spawned: the
/// scenario's mock backend plus the run's `env` (see `RunSetup::track_env`).
/// Set on `track` itself, or on an agent CLI whose children inherit it.
pub(crate) fn mock_track_env(cmd: &mut Command, scenario_path: &Path, env: &[(&str, String)]) {
    cmd.env(tracker_mock::MOCK_DIR_ENV, scenario_path);
    cmd.envs(env.iter().map(|(name, value)| (name, value)));
}

/// Route the agent's track commands to the scenario's mock backend with the
/// run's `env`, and put the track binary's directory first on PATH so a bare
/// `track` runs that binary.
pub(crate) fn route_track_to_mock(
    cmd: &mut Command,
    scenario_path: &Path,
    track_bin: &str,
    env: &[(&str, String)],
) {
    mock_track_env(cmd, scenario_path, env);

    if let Some(bin_dir) = Path::new(track_bin).parent()
        && !bin_dir.as_os_str().is_empty()
    {
        let current_path = std::env::var("PATH").unwrap_or_default();
        cmd.env("PATH", format!("{}:{}", bin_dir.display(), current_path));
    }
}

/// Reads a child's piped stderr on a background thread.
///
/// Nobody else reads it, so without this a child that writes more than a pipe
/// buffer (~64 KiB) to stderr blocks forever, while the harness blocks waiting
/// for its stdout or its exit.
pub(crate) struct StderrCapture(Option<JoinHandle<String>>);

impl StderrCapture {
    pub(crate) fn start(child: &mut Child) -> Self {
        Self(child.stderr.take().map(|mut stderr| {
            std::thread::spawn(move || {
                let mut bytes = Vec::new();
                // A read error only loses diagnostics; keep what was read.
                let _ = stderr.read_to_end(&mut bytes);
                String::from_utf8_lossy(&bytes).into_owned()
            })
        }))
    }

    /// Everything the child wrote to stderr, once it has closed it.
    fn finish(self) -> String {
        self.0
            .and_then(|reader| reader.join().ok())
            .unwrap_or_default()
    }
}

/// Fail if the CLI exited unsuccessfully, quoting the end of its stderr.
pub(crate) fn ensure_success(
    cli_name: &str,
    status: ExitStatus,
    stderr: StderrCapture,
) -> Result<()> {
    if status.success() {
        return Ok(());
    }
    let stderr = stderr.finish();
    let stderr = stderr.trim();
    if stderr.is_empty() {
        bail!("{cli_name} exited with {status}");
    }
    let tail_start = stderr.ceil_char_boundary(stderr.len().saturating_sub(STDERR_TAIL_BYTES));
    bail!("{cli_name} exited with {status}: {}", &stderr[tail_start..])
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::process::Stdio;

    fn env_of<'a>(cmd: &'a Command, name: &str) -> Option<&'a OsStr> {
        cmd.get_envs()
            .find(|(key, _)| *key == OsStr::new(name))
            .and_then(|(_, value)| value)
    }

    #[test]
    fn track_and_agent_cli_get_the_same_run_environment() {
        let env = [
            ("TRACK_CACHE_DIR", "/tmp/run-cache".to_string()),
            ("TRACK_OUTPUT", "agent".to_string()),
        ];
        let mut track = Command::new("track");
        mock_track_env(&mut track, Path::new("/scenarios/basic"), &env);
        let mut agent = Command::new("agent-cli");
        route_track_to_mock(
            &mut agent,
            Path::new("/scenarios/basic"),
            "/build/bin/track",
            &env,
        );

        for cmd in [&track, &agent] {
            assert_eq!(
                env_of(cmd, tracker_mock::MOCK_DIR_ENV),
                Some(OsStr::new("/scenarios/basic"))
            );
            assert_eq!(
                env_of(cmd, "TRACK_CACHE_DIR"),
                Some(OsStr::new("/tmp/run-cache"))
            );
            assert_eq!(env_of(cmd, "TRACK_OUTPUT"), Some(OsStr::new("agent")));
        }
        // Only the agent CLI needs `track` found on PATH.
        assert!(env_of(&track, "PATH").is_none());
        let path = env_of(&agent, "PATH").unwrap().to_str().unwrap();
        assert!(path.starts_with("/build/bin:"), "{path}");
    }

    fn run_sh(script: &str) -> Result<()> {
        let mut child = Command::new("sh")
            .args(["-c", script])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stderr = StderrCapture::start(&mut child);
        // Read stdout to EOF first, as the stream-json runners do.
        std::io::copy(&mut child.stdout.take().unwrap(), &mut std::io::sink())?;
        let status = child.wait()?;
        ensure_success("Fake CLI", status, stderr)
    }

    #[test]
    fn failure_quotes_the_end_of_stderr() {
        let err = run_sh("echo 'first line' >&2; echo 'auth failed' >&2; exit 3").unwrap_err();
        let message = err.to_string();
        assert!(
            message.starts_with("Fake CLI exited with exit status: 3"),
            "{message}"
        );
        assert!(message.ends_with("first line\nauth failed"), "{message}");
    }

    #[test]
    fn chatty_stderr_does_not_deadlock_the_reader() {
        // 1 MiB of stderr overflows any pipe buffer. Undrained, the child blocks
        // on stderr before writing stdout, and reading stdout never ends.
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = run_sh("yes | head -c 1048576 >&2; echo done; exit 1");
            let _ = done_tx.send(result.map_err(|e| e.to_string()));
        });
        let message = done_rx
            .recv_timeout(std::time::Duration::from_secs(60))
            .expect("child blocked on a full stderr pipe")
            .unwrap_err();
        assert!(
            message.starts_with("Fake CLI exited with exit status: 1"),
            "{message}"
        );
    }

    #[test]
    fn success_ignores_stderr() {
        assert!(run_sh("echo 'just a warning' >&2").is_ok());
    }
}
