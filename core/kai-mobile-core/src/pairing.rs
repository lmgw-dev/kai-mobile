//! Pairing over the device link (design §4; device link §3): a thin layer over
//! `lmgw_client::pairing` and `requests::enrol`.
//!
//! - [`preview`]: the link v2 parsed strictly (no I/O), and the facts the owner confirms;
//! - [`Enroller::enrol`]: the Keystore key, `POST /devices/enrol` over TLS pinned to the link's
//!   `fp`, raced on the link's URLs, the answer checked by lmgw-client's `read_enrolment`.
//!
//! **One key per token** (device link D8's lost answer): the enroller keeps the key it generated
//! for a token (by the token's SHA-256, never the token) until another link's enrolment replaces
//! it, so trying the same link again sends the same key, and lmgw's repeat answers the same
//! certificate instead of 410 `pairing_token_used`. Only a 4xx refusal carrying lmgw's code (other
//! than 429) deletes the key, since lmgw will not take this enrolment as it is; a 5xx, a 429 (the
//! second one too), an answer lmgw-client's own checks refuse, and a failure that reached no answer
//! keep it for the retry. Nothing is stored on any of them.
//!
//! The link is a credential while its token lives: nothing here logs it, and no error echoes a
//! value taken from it.

use std::{sync::Arc, time::Duration};

use lmgw_client::{
    pairing::{FactLine, PairFacts, PairingLink},
    requests::{self, Credentials, KeyRefused, code},
};

use crate::{
    CoreError, PairingFailure,
    keystore::{Keystore, fresh_alias},
    link::{
        failure::{Failure, UNPROVEN},
        race::race,
        tls::{Tls, describe, sha256},
    },
    views::{PairingPreview, PreviewLine},
};

/// The link `text` parsed (strict v2), and what the owner confirms; `current` names the pairing
/// this one would replace (its first URL).
pub fn preview(
    text: &str,
    current: Option<&str>,
) -> Result<(PairingLink, PairingPreview), CoreError> {
    let link = PairingLink::parse(text).map_err(|e| CoreError::BadLink {
        reason: e.to_string(),
    })?;
    let facts = PairFacts::new(&link, current);
    let lines = facts
        .lines()
        .into_iter()
        .map(|l| match l {
            FactLine::Plain(text) => PreviewLine {
                text,
                warning: false,
            },
            FactLine::Warning(text) => PreviewLine {
                text,
                warning: true,
            },
        })
        .collect();
    let preview = PairingPreview {
        name: facts.name,
        urls: facts.urls,
        fp: facts.fp,
        replaces: facts.replaces,
        lines,
    };
    Ok((link, preview))
}

/// A finished enrolment, nothing stored yet.
#[derive(Debug, Clone)]
pub struct Enrolled {
    pub alias: String,
    pub spki: Vec<u8>,
    pub credentials: Credentials,
    /// lmgw said the device is confirmed already (a repeat after the owner's Confirm).
    pub confirmed: bool,
}

/// The key kept for a token (module doc).
#[derive(Debug, Clone)]
struct PendingKey {
    token: [u8; 32],
    alias: String,
    spki: Vec<u8>,
}

/// Makes and keeps the enrolment's key (module doc).
pub struct Enroller {
    keystore: Arc<dyn Keystore>,
    /// Held across an enrolment: two Pair taps never race two enrolments.
    pending: tokio::sync::Mutex<Option<PendingKey>>,
}

impl Enroller {
    pub fn new(keystore: Arc<dyn Keystore>) -> Self {
        Self {
            keystore,
            pending: tokio::sync::Mutex::new(None),
        }
    }

    /// Forgets the key kept for a token (Forget gateway deleted it with the pairing).
    pub async fn clear(&self) {
        *self.pending.lock().await = None;
    }

    /// Enrols with `link` (module doc). `client` is the Devices card's text ("Kai mobile 0.1.0
    /// (Android 16)"), `timeout` `lmgw.timeout_s` (the whole race, and the answer's body),
    /// `stagger` `lmgw.url_stagger_ms`, `stored_alias` the stored pairing's key, which is never
    /// deleted here.
    pub async fn enrol(
        &self,
        link: &PairingLink,
        client: &str,
        timeout: Duration,
        stagger: Duration,
        stored_alias: Option<&str>,
    ) -> Result<Enrolled, CoreError> {
        let mut pending = self.pending.lock().await;
        let token = sha256(link.token.expose().as_bytes());
        let key = match pending.as_ref() {
            Some(p) if p.token == token => p.clone(),
            other => {
                if let Some(old) = other
                    && Some(old.alias.as_str()) != stored_alias
                {
                    self.keystore.delete(old.alias.clone());
                }
                let alias = fresh_alias();
                let spki =
                    self.keystore
                        .generate(alias.clone())
                        .map_err(|e| CoreError::Pairing {
                            kind: PairingFailure::Keystore,
                            reason: format!(
                                "the Android Keystore could not make this phone's key: {e}"
                            ),
                        })?;
                let key = PendingKey { token, alias, spki };
                *pending = Some(key.clone());
                key
            }
        };
        tracing::info!(alias = %key.alias, urls = link.urls.as_slice().len(), "pairing: enrolling");
        let mut retried = false;
        loop {
            match attempt(link, &key.spki, client, timeout, stagger).await {
                Ok(answer) => {
                    if answer.status == 429
                        && !retried
                        && let Some(wait) = answer.retry_after
                    {
                        // lmgw's own wait (pairing_rate_limited), then once more (§4.2).
                        tracing::info!(
                            secs = wait.as_secs(),
                            "pairing: rate limited; once more after Retry-After"
                        );
                        retried = true;
                        tokio::time::sleep(wait).await;
                        continue;
                    }
                    return match read(&answer, link, &key) {
                        Ok(enrolled) => Ok(enrolled),
                        Err(e) => {
                            // Nothing is stored; the new key goes only when lmgw refused this
                            // enrolment by its code (module doc, §4.2).
                            if refused_by_code(&answer) {
                                if Some(key.alias.as_str()) != stored_alias {
                                    self.keystore.delete(key.alias.clone());
                                }
                                *pending = None;
                            } else {
                                tracing::info!(
                                    status = answer.status,
                                    "pairing: the key is kept for the retry"
                                );
                            }
                            Err(e)
                        }
                    };
                }
                Err(e) => return Err(e),
            }
        }
    }
}

/// lmgw's answer to the enrolment.
struct Answer {
    url: String,
    status: u16,
    body: String,
    retry_after: Option<Duration>,
}

/// Whether lmgw refused the enrolment with a 4xx and its own code (not a 429): the only answer
/// after which the enrolment's key goes.
fn refused_by_code(answer: &Answer) -> bool {
    (400..500).contains(&answer.status)
        && answer.status != 429
        && !requests::read_refusal(answer.status, &answer.body)
            .code
            .starts_with("http_")
}

/// One race of the enrolment over the link's URLs.
async fn attempt(
    link: &PairingLink,
    spki: &[u8],
    client: &str,
    timeout: Duration,
    stagger: Duration,
) -> Result<Answer, CoreError> {
    let tls = Tls::enrolment(link.fp, timeout).map_err(|reason| CoreError::Pairing {
        kind: PairingFailure::Unreachable,
        reason,
    })?;
    // The request carries the token: never logged.
    let req = requests::enrol(&link.token, spki, Some(client));
    let attempt = |base: &str, answered| {
        let http = tls.racing(answered);
        let url = req.url(base);
        let req = &req;
        async move {
            let http = http.map_err(|e| Failure::Other(describe(&e)))?;
            let mut rb = http.post(url);
            for h in &req.headers {
                rb = rb.header(h.name.as_str(), h.value.as_str());
            }
            let resp = rb
                .body(req.body.clone().unwrap_or_default())
                .send()
                .await
                .map_err(|e| Failure::of_enrolment(&e))?;
            let status = resp.status().as_u16();
            let retry_after = resp
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse::<u64>().ok())
                .map(Duration::from_secs);
            let body = tokio::time::timeout(timeout, resp.text())
                .await
                .map_err(|_| Failure::NoAnswer)?
                .map_err(|e| Failure::Other(format!("the answer broke off: {}", describe(&e))))?;
            Ok::<_, Failure>((status, body, retry_after))
        }
    };
    let urls = link.urls.order(None);
    let raced = tokio::time::timeout(timeout, race(urls, stagger, attempt)).await;
    match raced {
        Err(_) => Err(CoreError::Pairing {
            kind: PairingFailure::Unreachable,
            reason: format!(
                "lmgw did not answer the enrolment within {} s (lmgw.timeout_s)",
                timeout.as_secs()
            ),
        }),
        Ok(Ok((url, (status, body, retry_after)))) => {
            tracing::info!(%url, status, "pairing: lmgw answered the enrolment");
            Ok(Answer {
                url,
                status,
                body,
                retry_after,
            })
        }
        Ok(Err(failed)) => Err(unanswered(&failed)),
    }
}

/// Every URL failed without an answer: the first fitting kind, each URL's words.
fn unanswered(failed: &[(String, Failure)]) -> CoreError {
    let kind = if failed
        .iter()
        .any(|(_, f)| *f == Failure::Tls(lmgw_client::tls::TlsRefusal::NotPairedLmgw))
    {
        PairingFailure::NotThisLmgw
    } else if failed.iter().any(|(_, f)| *f == Failure::Unproven) {
        PairingFailure::Unproven
    } else {
        PairingFailure::Unreachable
    };
    let reason = match (kind, failed) {
        (PairingFailure::NotThisLmgw, _) => format!(
            "This is not the lmgw this link pairs with: nothing was sent to it. {}",
            each(failed)
        ),
        (PairingFailure::Unproven, [_]) => UNPROVEN.to_owned(),
        (_, [(_, f)]) => format!("lmgw is not reachable: {}", f.words()),
        _ => format!(
            "lmgw did not answer at any of the link's addresses: {}",
            each(failed)
        ),
    };
    CoreError::Pairing { kind, reason }
}

fn each(failed: &[(String, Failure)]) -> String {
    failed
        .iter()
        .map(|(u, f)| format!("{u}: {}", f.words()))
        .collect::<Vec<_>>()
        .join("; ")
}

/// lmgw's answer read: the credentials, or the refusal in §5.3's words.
fn read(answer: &Answer, link: &PairingLink, key: &PendingKey) -> Result<Enrolled, CoreError> {
    match requests::read_enrolment(answer.status, &answer.body, link, &key.spki) {
        Ok(credentials) => {
            let confirmed = serde_json::from_str::<requests::enrol::Enrolment>(&answer.body)
                .map(|e| e.confirmed)
                .unwrap_or(false);
            tracing::info!(url = %answer.url, device = credentials.device_id, confirmed, "pairing: enrolled");
            Ok(Enrolled {
                alias: key.alias.clone(),
                spki: key.spki.clone(),
                credentials,
                confirmed,
            })
        }
        Err(e) => {
            tracing::warn!(code = %e.code, status = answer.status, "pairing: lmgw refused the enrolment");
            let name = lmgw_client::pairing::display_name(&link.name);
            let (kind, reason) = match (requests::key_refused(&e), e.code.as_str()) {
                (Some(KeyRefused::TokenUsedElsewhere), _) => (
                    PairingFailure::CodeUsedElsewhere,
                    format!(
                        "Someone else used this pairing code. Delete the device '{name}' on lmgw's \
                         Devices card and pair with a new link. (lmgw: {})",
                        e.message
                    ),
                ),
                (_, code::PAIRING_TOKEN_EXPIRED) => (
                    PairingFailure::LinkExpired,
                    format!(
                        "This pairing link expired (devices.token_minutes on lmgw). Make a new one \
                         on the Devices card. (lmgw: {})",
                        e.message
                    ),
                ),
                (_, code::PAIRING_TOKEN_UNKNOWN) => (
                    PairingFailure::LinkUnknown,
                    "lmgw does not know this pairing link. Make a new one on the Devices card."
                        .to_owned(),
                ),
                (
                    _,
                    code::CA_NOT_PINNED
                    | code::CERTIFICATE_NOT_FOR_KEY
                    | code::CONFIRM_CODE_MISMATCH,
                ) => (PairingFailure::NotThisLmgw, e.message.clone()),
                _ => (
                    PairingFailure::Refused,
                    format!("lmgw refused the enrolment: {} ({})", e.message, e.code),
                ),
            };
            Err(CoreError::Pairing { kind, reason })
        }
    }
}
