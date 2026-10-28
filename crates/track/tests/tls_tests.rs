//! track trusts its built-in Mozilla roots, plus the certificates named by
//! `SSL_CERT_FILE` / `SSL_CERT_DIR`, plus the operating system's store when
//! `system_certs` is enabled.
//!
//! The server here presents a certificate from an authority created for the
//! test, as a proxy that inspects HTTPS does.

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use rcgen::{BasicConstraints, CertificateParams, DnType, IsCa, KeyPair};
use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

const ISSUE_JSON: &str = r#"{"id":"1-1","idReadable":"DEMO-1","summary":"Behind the proxy","project":{"id":"0-1","shortName":"DEMO"},"customFields":[],"tags":[],"created":1640000000000,"updated":1640000000000}"#;

/// A YouTrack stand-in serving `DEMO-1` over HTTPS with a certificate issued
/// by its own certificate authority, and a directory holding that
/// authority's certificate as `ca.pem`.
struct InspectingProxy {
    port: u16,
    dir: PathBuf,
}

impl InspectingProxy {
    fn start() -> Self {
        let ca_key = KeyPair::generate().unwrap();
        let mut ca_params = CertificateParams::new(Vec::<String>::new()).unwrap();
        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        ca_params
            .distinguished_name
            .push(DnType::CommonName, "track test proxy CA");
        let ca = ca_params.self_signed(&ca_key).unwrap();

        let server_key = KeyPair::generate().unwrap();
        let server = CertificateParams::new(vec!["127.0.0.1".to_string()])
            .unwrap()
            .signed_by(&server_key, &ca, &ca_key)
            .unwrap();
        let server_config = Arc::new(
            rustls::ServerConfig::builder_with_provider(Arc::new(
                rustls::crypto::ring::default_provider(),
            ))
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(
                vec![server.der().clone()],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(server_key.serialize_der())),
            )
            .unwrap(),
        );

        let dir = tempfile::tempdir().unwrap().keep();
        std::fs::create_dir_all(dir.join("certs")).unwrap();
        std::fs::write(dir.join("certs").join("ca.pem"), ca.pem()).unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        thread::spawn(move || {
            for tcp in listener.incoming().flatten() {
                let connection = rustls::ServerConnection::new(server_config.clone()).unwrap();
                thread::spawn(move || respond(rustls::StreamOwned::new(connection, tcp)));
            }
        });
        Self { port, dir }
    }

    fn ca_file(&self) -> PathBuf {
        self.dir.join("certs").join("ca.pem")
    }

    fn ca_dir(&self) -> PathBuf {
        self.dir.join("certs")
    }

    /// `track issue get DEMO-1` against this server, with no certificate
    /// settings beyond the ones a test adds.
    fn track(&self) -> assert_cmd::Command {
        let mut cmd = cargo_bin_cmd!("track");
        cmd.current_dir(&self.dir)
            .env("HOME", &self.dir)
            .env("USERPROFILE", &self.dir)
            .args(["-b", "youtrack", "--url"])
            .arg(format!("https://127.0.0.1:{}", self.port))
            .args(["--token", "test-token", "issue", "get", "DEMO-1"]);
        for var in [
            "SSL_CERT_FILE",
            "SSL_CERT_DIR",
            "TRACKER_SYSTEM_CERTS",
            "TRACKER_CONFIG",
            "TRACKER_BACKEND",
            "TRACKER_URL",
            "TRACKER_TOKEN",
            "YOUTRACK_URL",
            "YOUTRACK_TOKEN",
            "TRACK_MOCK_DIR",
        ] {
            cmd.env_remove(var);
        }
        cmd
    }
}

impl Drop for InspectingProxy {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Answer one request with `DEMO-1`. A client that rejects the certificate
/// ends the handshake, which ends this read.
fn respond(mut tls: rustls::StreamOwned<rustls::ServerConnection, std::net::TcpStream>) {
    let mut request = Vec::new();
    let mut chunk = [0; 1024];
    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
        match tls.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(n) => request.extend_from_slice(&chunk[..n]),
        }
    }
    let _ = write!(
        tls,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{ISSUE_JSON}",
        ISSUE_JSON.len()
    );
    tls.conn.send_close_notify();
    let _ = tls.flush();
}

fn assert_untrusted(cmd: &mut assert_cmd::Command) -> assert_cmd::assert::Assert {
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("UnknownIssuer"))
        .stderr(predicate::str::contains("Tip:"))
        .stderr(predicate::str::contains("SSL_CERT_FILE"))
        .stderr(predicate::str::contains(
            "track config set system_certs true",
        ))
}

#[test]
fn an_unknown_certificate_authority_is_rejected_by_default() {
    let proxy = InspectingProxy::start();
    assert_untrusted(&mut proxy.track());
}

#[test]
fn ssl_cert_file_adds_its_certificate_authority() {
    let proxy = InspectingProxy::start();
    proxy
        .track()
        .env("SSL_CERT_FILE", proxy.ca_file())
        .assert()
        .success()
        .stdout(predicate::str::contains("Behind the proxy"));
}

#[test]
fn ssl_cert_dir_adds_its_certificate_authorities() {
    let proxy = InspectingProxy::start();
    proxy
        .track()
        .env("SSL_CERT_DIR", proxy.ca_dir())
        .assert()
        .success()
        .stdout(predicate::str::contains("Behind the proxy"));
}

#[test]
fn an_unreadable_ssl_cert_file_keeps_the_builtin_roots_and_says_why() {
    let proxy = InspectingProxy::start();
    let missing = proxy.dir.join("missing.pem");
    assert_untrusted(proxy.track().env("SSL_CERT_FILE", &missing))
        .stderr(predicate::str::contains("Could not load certificates"))
        .stderr(predicate::str::contains(path_text(&missing)));
}

#[test]
fn system_certs_reads_the_os_store_without_trusting_unknown_authorities() {
    // The test authority is not in the OS store, so the request still fails,
    // after loading the platform's certificates without error.
    let proxy = InspectingProxy::start();
    assert_untrusted(proxy.track().env("TRACKER_SYSTEM_CERTS", "true"));
}

#[test]
fn system_certs_defers_to_ssl_cert_file() {
    let proxy = InspectingProxy::start();
    proxy
        .track()
        .env("TRACKER_SYSTEM_CERTS", "true")
        .env("SSL_CERT_FILE", proxy.ca_file())
        .assert()
        .success();
}

#[test]
fn system_certs_is_a_config_key() {
    let proxy = InspectingProxy::start();
    let config = |args: &[&str]| {
        let mut cmd = cargo_bin_cmd!("track");
        cmd.current_dir(&proxy.dir)
            .env("HOME", &proxy.dir)
            .env("USERPROFILE", &proxy.dir)
            .env_remove("TRACKER_SYSTEM_CERTS")
            .env_remove("TRACKER_CONFIG")
            .arg("config")
            .args(args);
        cmd
    };

    config(&["set", "system_certs", "true"]).assert().success();
    config(&["get", "system_certs"])
        .assert()
        .success()
        .stdout(predicate::str::contains("true"));
    config(&["set", "system_certs", "maybe"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Valid: true, false"));
    config(&["keys"])
        .assert()
        .success()
        .stdout(predicate::str::contains("system_certs"));
}

fn path_text(path: &Path) -> String {
    path.display().to_string()
}
