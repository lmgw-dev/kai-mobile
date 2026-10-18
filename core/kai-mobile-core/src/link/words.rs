//! The connection's words (design §5.3, §5.5): what each state says and which actions it offers,
//! and how a race's failures read as one state, first match wins.

use lmgw_client::tls::TlsRefusal;

use super::failure::Failure;
use crate::views::{ConnectionAction as A, ConnectionState as S, KeyRefusal};

/// A state with its words, as the actor keeps it before the badges join.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Said {
    pub state: S,
    pub words: String,
    pub detail: Vec<String>,
    pub actions: Vec<A>,
    pub refusal: Option<KeyRefusal>,
}

impl Said {
    pub fn new(state: S, words: impl Into<String>, actions: Vec<A>) -> Self {
        Self {
            state,
            words: words.into(),
            detail: Vec::new(),
            actions,
            refusal: None,
        }
    }

    pub fn not_paired() -> Self {
        Self::new(S::NotPaired, "Not paired with lmgw", vec![A::Pair])
    }

    pub fn connecting() -> Self {
        Self::new(S::Connecting, "Connecting to lmgw…", vec![])
    }

    pub fn no_network() -> Self {
        Self::new(S::NoNetwork, "The phone has no network.", vec![])
    }

    pub fn unconfirmed(code: &str) -> Self {
        Self::new(
            S::WaitingForConfirmation,
            format!("Confirm {code} on lmgw's Devices card."),
            vec![A::CheckNow, A::PairAgain],
        )
    }

    pub fn connected(name: &str) -> Self {
        Self::new(
            S::Connected,
            format!("Connected to lmgw as '{name}'"),
            vec![],
        )
    }

    pub fn lapsed(date: &str) -> Self {
        Self::new(
            S::CertificateLapsed,
            format!(
                "This device's certificate lapsed on {date}. Pair it again (New token… on lmgw's \
                 Devices card)."
            ),
            vec![A::PairAgain],
        )
    }

    pub fn key_missing(why: &str) -> Self {
        Self::new(
            S::KeyMissing,
            format!(
                "This phone's key for lmgw is gone from the Android Keystore ({why}): pair again."
            ),
            vec![A::PairAgain],
        )
    }

    /// A refused key (§5.5), by kind; `message` is lmgw's sentence.
    pub fn refused(kind: KeyRefusal, code: &str, message: &str) -> Self {
        let (words, actions) = match kind {
            KeyRefusal::PairAgain => (
                format!(
                    "lmgw no longer knows this device's key (paired again elsewhere, or deleted): \
                     pair again. (lmgw: {message})"
                ),
                vec![A::PairAgain],
            ),
            KeyRefusal::Disabled => (
                "This device was disabled on lmgw: enable it on its Devices card, or pair again."
                    .to_owned(),
                vec![A::TryAgain, A::PairAgain],
            ),
            KeyRefusal::Expired => (
                "This device's access expired on lmgw: move its expiry on its Devices card, or \
                 pair again."
                    .to_owned(),
                vec![A::TryAgain, A::PairAgain],
            ),
            KeyRefusal::Bug => (
                format!(
                    "lmgw refused the phone's request ({code}: {message}). That is a bug in Kai, \
                     not yours to fix: please report it with the log. Kai tries again."
                ),
                vec![A::TryAgain],
            ),
            KeyRefusal::Unknown => (
                format!(
                    "lmgw refused this device ({code}: {message}). Kai keeps the pairing and tries \
                     again."
                ),
                vec![A::TryAgain],
            ),
        };
        Self {
            refusal: Some(kind),
            ..Self::new(S::KeyRefused, words, actions)
        }
    }

    pub fn unreachable(words: impl Into<String>) -> Self {
        Self::new(S::Unreachable, words, vec![A::TryAgain])
    }
}

/// The host of an `https://host[:port][/path]` URL.
pub fn host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let hostport = rest.split('/').next().unwrap_or(rest);
    match hostport.rsplit_once(':') {
        // An IPv6 literal keeps its brackets; a port goes.
        Some((h, port)) if port.chars().all(|c| c.is_ascii_digit()) && !h.is_empty() => h,
        _ => hostport,
    }
}

/// A race's failures as one state, first match wins (§5.3); `timeout_s` names the bound.
/// `None` when the failures name no state of their own (the caller says what broke).
pub fn of_failures(failures: &[(String, Failure)], timeout_s: u64) -> (Said, bool) {
    let detail: Vec<String> = failures
        .iter()
        .map(|(u, f)| format!("{u}: {}", f.words()))
        .collect();
    let with_detail = |mut s: Said| {
        s.detail = detail.clone();
        s
    };
    // The key is gone for good: the pairing is useless (stop).
    if let Some((_, Failure::KeyGone(why))) = failures
        .iter()
        .find(|(_, f)| matches!(f, Failure::KeyGone(_)))
    {
        return (with_detail(Said::key_missing(why)), true);
    }
    // The Keystore failed otherwise: said and tried again on the backoff.
    if let Some((_, Failure::KeyFailed(why))) = failures
        .iter()
        .find(|(_, f)| matches!(f, Failure::KeyFailed(_)))
    {
        return (
            with_detail(Said::unreachable(format!(
                "The Android Keystore could not sign for this phone's key ({why}). Kai keeps the \
                 pairing and tries again."
            ))),
            false,
        );
    }
    if failures
        .iter()
        .any(|(_, f)| *f == Failure::Tls(TlsRefusal::AuthorityChanged))
    {
        return (
            with_detail(Said::new(
                S::NotThisLmgw,
                "lmgw's device authority changed, or this is not your lmgw: pair again.",
                vec![A::PairAgain, A::TryAgain],
            )),
            false,
        );
    }
    let quiet = |f: &Failure| matches!(f, Failure::Refused | Failure::NoAnswer);
    let unnamed: Vec<String> = failures
        .iter()
        .filter_map(|(u, f)| match f {
            Failure::Tls(TlsRefusal::HostNotNamed { host: h }) => {
                Some(h.clone().unwrap_or_else(|| host(u).to_owned()))
            }
            _ => None,
        })
        .collect();
    if !unnamed.is_empty()
        && failures
            .iter()
            .all(|(_, f)| quiet(f) || matches!(f, Failure::Tls(TlsRefusal::HostNotNamed { .. })))
    {
        return (
            with_detail(Said::new(
                S::HostNotNamed,
                format!(
                    "lmgw's certificate does not name {}: add this URL to the device's row on \
                     lmgw's Devices card.",
                    unnamed.join(", ")
                ),
                vec![A::TryAgain],
            )),
            false,
        );
    }
    if !failures.is_empty() && failures.iter().all(|(_, f)| quiet(f)) {
        let refused: Vec<&str> = failures
            .iter()
            .filter(|(_, f)| *f == Failure::Refused)
            .map(|(u, _)| host(u))
            .collect();
        if !refused.is_empty() {
            return (
                with_detail(Said::new(
                    S::NotRunning,
                    format!(
                        "lmgw is not running on {}, or its device listener is off.",
                        refused.join(" or ")
                    ),
                    vec![A::TryAgain],
                )),
                false,
            );
        }
        let hosts: Vec<&str> = failures.iter().map(|(u, _)| host(u)).collect();
        return (
            with_detail(Said::new(
                S::NotAnswering,
                format!(
                    "The computer with lmgw does not answer at {} (asleep, off, or out of reach of \
                     its network, e.g. away without the VPN). Gave up after {timeout_s} s (lmgw.timeout_s).",
                    hosts.join(" or ")
                ),
                vec![A::TryAgain, A::OpenWireGuard],
            )),
            false,
        );
    }
    let words = match failures {
        [(_, f)] => format!("lmgw is not reachable: {}", f.words()),
        _ => "lmgw is not reachable at any of its addresses.".to_owned(),
    };
    (with_detail(Said::unreachable(words)), false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_are_read_off_urls() {
        assert_eq!(host("https://198.51.100.20:8788"), "198.51.100.20");
        assert_eq!(host("https://lmgw.example/prefix"), "lmgw.example");
        assert_eq!(host("https://[2001:db8::1]:8788"), "[2001:db8::1]");
    }

    #[test]
    fn failures_read_first_match_wins() {
        let lan = "https://198.51.100.20:8788".to_owned();
        let vpn = "https://192.0.2.1:8788".to_owned();
        let (s, _) = of_failures(
            &[
                (lan.clone(), Failure::Refused),
                (vpn.clone(), Failure::NoAnswer),
            ],
            10,
        );
        assert_eq!(s.state, S::NotRunning);
        assert!(s.words.contains("198.51.100.20"), "{}", s.words);
        let (s, _) = of_failures(
            &[
                (lan.clone(), Failure::NoAnswer),
                (vpn.clone(), Failure::NoAnswer),
            ],
            10,
        );
        assert_eq!(s.state, S::NotAnswering);
        assert!(s.words.contains("10 s (lmgw.timeout_s)"));
        assert_eq!(s.detail.len(), 2);
        let (s, _) = of_failures(
            &[
                (
                    lan.clone(),
                    Failure::Tls(TlsRefusal::HostNotNamed { host: None }),
                ),
                (vpn.clone(), Failure::NoAnswer),
            ],
            10,
        );
        assert_eq!(s.state, S::HostNotNamed);
        let (s, stop) = of_failures(
            &[(lan.clone(), Failure::Alert("bad_certificate".into()))],
            10,
        );
        assert_eq!(
            (s.state, stop),
            (S::Unreachable, false),
            "an alert is said and tried again"
        );
        assert!(s.words.contains("bad_certificate"));
        let (s, stop) = of_failures(&[(lan.clone(), Failure::KeyGone("gone".into()))], 10);
        assert_eq!((s.state, stop), (S::KeyMissing, true));
        let (s, stop) = of_failures(&[(lan, Failure::KeyFailed("busy".into()))], 10);
        assert_eq!((s.state, stop), (S::Unreachable, false), "only gone stops");
    }
}
