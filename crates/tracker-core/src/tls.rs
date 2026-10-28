//! Certificate authorities trusted for HTTPS connections to trackers.
//!
//! By default track trusts the Mozilla root certificates built into the
//! binary, the same set on every machine. A proxy that inspects HTTPS
//! re-signs traffic with its own certificate authority, which that set lacks.
//! Two settings add certificates to it, and neither removes any:
//!
//! - `SSL_CERT_FILE` / `SSL_CERT_DIR`, the variables OpenSSL-based tools
//!   read, name extra PEM certificates;
//! - `system_certs` ([`set_system_certs`]) adds the operating system's
//!   certificate store when those variables are unset.
//!
//! With neither, clients use exactly the built-in roots.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use ureq::tls::{Certificate, RootCerts, TlsConfig};

static SYSTEM_CERTS: AtomicBool = AtomicBool::new(false);

/// Also trust the operating system's certificate store. The trusted roots are
/// resolved once, when the first client is built, so call this before that.
pub fn set_system_certs(enabled: bool) {
    SYSTEM_CERTS.store(enabled, Ordering::Relaxed);
}

/// TLS settings for every tracker client.
pub fn tls_config() -> TlsConfig {
    TlsConfig::builder()
        .root_certs(trusted_roots().certs.clone())
        .build()
}

/// Why certificates requested through `SSL_CERT_FILE`, `SSL_CERT_DIR` or
/// `system_certs` could not be loaded. Empty when none were requested.
pub fn load_errors() -> &'static [String] {
    &trusted_roots().load_errors
}

struct TrustedRoots {
    certs: RootCerts,
    load_errors: Vec<String>,
}

fn trusted_roots() -> &'static TrustedRoots {
    static ROOTS: OnceLock<TrustedRoots> = OnceLock::new();
    ROOTS.get_or_init(|| {
        let (additional, load_errors) = load_additional_certs();
        TrustedRoots {
            certs: with_builtin_roots(additional),
            load_errors,
        }
    })
}

/// Certificates to trust besides the built-in roots: those named by
/// `SSL_CERT_FILE` / `SSL_CERT_DIR` when set, else the operating system's
/// store when `system_certs` is enabled, else none.
fn load_additional_certs() -> (Vec<Certificate<'static>>, Vec<String>) {
    let variables_set = ["SSL_CERT_FILE", "SSL_CERT_DIR"]
        .iter()
        .any(|variable| std::env::var_os(variable).is_some_and(|value| !value.is_empty()));
    if !variables_set && !SYSTEM_CERTS.load(Ordering::Relaxed) {
        return (Vec::new(), Vec::new());
    }
    // Reads the variables when they are set, the operating system's store
    // otherwise.
    let loaded = rustls_native_certs::load_native_certs();
    let certs = loaded
        .certs
        .iter()
        .map(|cert| Certificate::from_der(cert).to_owned())
        .collect();
    let errors = loaded.errors.iter().map(ToString::to_string).collect();
    (certs, errors)
}

/// The built-in Mozilla roots plus `additional`.
fn with_builtin_roots(additional: Vec<Certificate<'static>>) -> RootCerts {
    if additional.is_empty() {
        // ureq's own copy of the Mozilla roots, which also keeps the name
        // constraints Mozilla applies on top of the certificates.
        return RootCerts::WebPki;
    }
    // The same roots as whole certificates, so `additional` can join them.
    // Like any system or `SSL_CERT_FILE` bundle, they lack Mozilla's applied
    // name constraints (one root, limited to `.tr` domains).
    let builtin = webpki_root_certs::TLS_SERVER_ROOT_CERTS
        .iter()
        .map(|cert| Certificate::from_der(cert));
    RootCerts::Specific(Arc::new(builtin.chain(additional).collect()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_additional_certs_the_builtin_roots_are_unchanged() {
        assert!(matches!(with_builtin_roots(Vec::new()), RootCerts::WebPki));
    }

    #[test]
    fn additional_certs_join_every_builtin_root() {
        let additional = Certificate::from_der(b"proxy CA").to_owned();
        let RootCerts::Specific(certs) = with_builtin_roots(vec![additional.clone()]) else {
            panic!("expected a specific root set");
        };

        assert_eq!(certs.last().unwrap().der(), additional.der());
        // Every root ureq trusts by default is still trusted.
        let contains = |haystack: &[u8], needle: &[u8]| {
            haystack
                .windows(needle.len())
                .any(|window| window == needle)
        };
        for anchor in webpki_roots::TLS_SERVER_ROOTS {
            assert!(
                certs
                    .iter()
                    .any(|cert| contains(cert.der(), &anchor.subject)
                        && contains(cert.der(), &anchor.subject_public_key_info)),
                "built-in root missing: {:?}",
                anchor.subject
            );
        }
    }
}
