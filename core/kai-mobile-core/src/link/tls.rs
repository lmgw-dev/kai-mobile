//! TLS from one place (design M2, M7, §4.4; device link D15): rustls with the ring provider, never
//! aws-lc-rs, so the NDK build has no C library of its own to compile. Every connection the core
//! makes to lmgw (the enrolment, the feed, each request, the send stream) is built by [`Tls`] from
//! one `rustls::ClientConfig`:
//!
//! - a paired device's ([`Tls::paired`]): lmgw-client's `client_config`, lmgw's device CA as the
//!   only root and the device's certificate signed through the Keystore callback, TLS 1.3 only;
//! - the enrolment's ([`Tls::enrolment`]): lmgw-client's `enrol_config`, pinned to the link's `fp`,
//!   no client certificate.
//!
//! reqwest is handed the configuration whole (`tls_backend_preconfigured`): https only, no proxy,
//! no redirects, no platform verifier and no bundled roots (the system's trust store has no say,
//! and nothing goes over plain http). `clippy.toml` refuses `reqwest::Client::new`, `builder` and
//! the `ClientBuilder` type everywhere but [`builder`] here, and this module's test refuses
//! `Client::default` in the sources: a default client would also use the platform verifier, which
//! panics on Android where it is never initialised.

use std::{sync::Arc, time::Duration};

use lmgw_client::{pairing::Fingerprint, requests::Credentials};
use rustls::{ClientConfig, crypto::CryptoProvider, sign::SigningKey};

use super::race::{Answered, OnConnect};

/// The provider of the whole core: ring.
pub fn provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// The one client configuration and what is built from it (module doc). Cheap to clone; clones
/// share the HTTP client's pool.
#[derive(Clone)]
pub struct Tls {
    config: Arc<ClientConfig>,
    http: reqwest::Client,
    /// `lmgw.timeout_s`, each connection's TCP and TLS.
    connect_timeout: Duration,
    /// Which pairing this is: the SHA-256 of the device's certificate (all zero for the
    /// enrolment's). A refusal is acted on only when it came under the current pairing's identity
    /// (a request of the old pairing must not tear down a new one after a re-pair).
    identity: [u8; 32],
}

impl std::fmt::Debug for Tls {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tls").finish_non_exhaustive()
    }
}

impl Tls {
    fn with_identity(
        config: ClientConfig,
        connect_timeout: Duration,
        identity: [u8; 32],
    ) -> Result<Self, String> {
        let http = builder(&config, connect_timeout)
            .build()
            .map_err(|e| format!("cannot build the HTTP client for lmgw: {}", describe(&e)))?;
        Ok(Self {
            config: Arc::new(config),
            http,
            connect_timeout,
            identity,
        })
    }

    /// A paired device's (module doc). Refused when the stored CA does not read as a root, or the
    /// key is not the certificate's.
    pub fn paired(
        stored: &Credentials,
        key: Arc<dyn SigningKey>,
        connect_timeout: Duration,
    ) -> Result<Self, String> {
        let config = lmgw_client::tls::client_config(provider(), stored, key)
            .map_err(|e| format!("the stored pairing does not hold together: {e}"))?;
        let identity = sha256(&stored.certificate);
        Self::with_identity(config, connect_timeout, identity)
    }

    /// The enrolment's (module doc).
    pub fn enrolment(fp: Fingerprint, connect_timeout: Duration) -> Result<Self, String> {
        let config = lmgw_client::tls::enrol_config(provider(), fp)
            .map_err(|e| format!("cannot set up TLS for the enrolment: {e}"))?;
        Self::with_identity(config, connect_timeout, [0; 32])
    }

    /// The same configuration with a pool of its own and `connect_timeout`: what a change of the
    /// default network makes (design §5.2: the pooled connections are dropped), or a changed
    /// `lmgw.timeout_s`.
    pub fn fresh(&self, connect_timeout: Duration) -> Result<Self, String> {
        Self::with_identity((*self.config).clone(), connect_timeout, self.identity)
    }

    /// The HTTP client (a handle: clones share its connections).
    pub fn http(&self) -> reqwest::Client {
        self.http.clone()
    }

    /// An HTTP client of this configuration for one attempt of the URL race, with connections of
    /// its own: `answered` says when its connection is up (TCP and the TLS handshake).
    pub fn racing(&self, answered: Answered) -> Result<reqwest::Client, reqwest::Error> {
        builder(&self.config, self.connect_timeout)
            .connector_layer(OnConnect(answered))
            .build()
    }

    /// Which pairing this is (field doc).
    pub fn identity(&self) -> [u8; 32] {
        self.identity
    }
}

/// The one place a reqwest client is built (module doc): `config` as its TLS, https only, no
/// proxy (the gateway is talked to directly, and a proxy would hold back the feed), no redirects
/// (an answer that points elsewhere is lmgw's answer, never followed), TCP and TLS within
/// `connect_timeout`.
#[allow(clippy::disallowed_methods, clippy::disallowed_types)]
fn builder(config: &ClientConfig, connect_timeout: Duration) -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .no_proxy()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(connect_timeout)
        .tls_backend_preconfigured(config.clone())
}

/// SHA-256 of `bytes`.
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    let d = ring::digest::digest(&ring::digest::SHA256, bytes);
    let mut out = [0u8; 32];
    out.copy_from_slice(d.as_ref());
    out
}

/// An error and its causes, down to the one that says what happened ("Connection refused").
pub fn describe(e: &(dyn std::error::Error + 'static)) -> String {
    let mut last = e.to_string();
    let mut cur = e.source();
    while let Some(s) = cur {
        let text = s.to_string();
        if !text.is_empty() {
            last = text;
        }
        cur = s.source();
    }
    last
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    /// clippy names no trait impl's method, so `clippy.toml` cannot refuse
    /// `reqwest::Client::default()`: no source of the crate (its tests included) spells it, nor
    /// a `reqwest::Client` from `Default::default()` (comments aside).
    #[test]
    fn no_source_builds_a_default_reqwest_client() {
        fn walk(dir: &Path, found: &mut Vec<String>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for e in entries.flatten() {
                let path = e.path();
                if path.is_dir() {
                    walk(&path, found);
                } else if path.extension().is_some_and(|x| x == "rs") {
                    let text = std::fs::read_to_string(&path).unwrap_or_default();
                    for (n, line) in text.lines().enumerate() {
                        let code = line.split("//").next().unwrap_or_default();
                        let spelled = code.contains(concat!("Client", "::default"))
                            || code.contains(concat!("Client as ", "Default"))
                            || (code.contains(concat!("reqwest::", "Client"))
                                && code.contains(concat!("Default::", "default")));
                        if spelled {
                            found.push(format!("{}:{}", path.display(), n + 1));
                        }
                    }
                }
            }
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut found = Vec::new();
        walk(&root.join("src"), &mut found);
        walk(&root.join("tests"), &mut found);
        assert!(
            found.is_empty(),
            "a default reqwest client (device link D15: link::tls::Tls builds every one): {found:?}"
        );
    }
}
