//! Why a connection to lmgw failed (design §5.2, §5.3, §4.4), each URL's failure kept for the
//! state's words: refused, no answer, a host the certificate does not name, an unknown issuer, or
//! the error's own words.
//!
//! **An alert is never a refusal** (Kai desktop's security review B1). rustls takes a fatal alert
//! in any state of the handshake, a plaintext one right after the ClientHello included, before the
//! server proved anything, so whoever answers at an address (a squatter on lmgw's port, a stranger
//! on a foreign Wi-Fi) can send `bad_certificate`, `unknown_ca` or `certificate_expired`. Such an
//! alert is said ([`Failure::Alert`]) and tried again on the backoff; a lapse is decided by the
//! app's own date, and lmgw's real refusals come over the authenticated link (a 401, a 403
//! `device_unconfirmed`, the feed's `revoked`). At enrolment the `certificate_required` alert is
//! lmgw-client's `TlsRefusal::Unproven`: a failed handshake to retry.

use lmgw_client::tls::{TlsRefusal, find_tls_error};

use super::tls::describe;
use crate::keystore::{SIGN_FAILED, SIGN_GONE};

/// One URL's failure (module doc).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// The connection was refused: nothing listens there (lmgw not running, its device listener
    /// off).
    Refused,
    /// No TCP answer within the bound (asleep, off, out of reach without the VPN).
    NoAnswer,
    /// lmgw-client's reading of a refused handshake of lmgw's side: an unknown issuer, a host the
    /// certificate does not name, the server's dates, or at enrolment "not the lmgw this link
    /// pairs with".
    Tls(TlsRefusal),
    /// A TLS alert about the device's certificate (TLS's name of it), from whoever answered,
    /// before it proved to be lmgw: said and tried again.
    Alert(String),
    /// At enrolment: lmgw-client's `TlsRefusal::Unproven` (a `certificate_required` alert).
    Unproven,
    /// The Keystore's device key is gone for good (invalidated, unrecoverable, no entry).
    KeyGone(String),
    /// The Keystore could not sign for another reason (busy, a binder error): tried again.
    KeyFailed(String),
    /// Anything else, in the error's own words.
    Other(String),
}

impl Failure {
    /// A paired connection's error (`TlsRefusal::of`).
    pub fn of(e: &(dyn std::error::Error + 'static)) -> Self {
        classify(e, false)
    }

    /// An enrolment's error (`TlsRefusal::of_enrolment`).
    pub fn of_enrolment(e: &(dyn std::error::Error + 'static)) -> Self {
        classify(e, true)
    }

    /// The words for one URL.
    pub fn words(&self) -> String {
        match self {
            Self::Refused => "the connection was refused".to_owned(),
            Self::NoAnswer => "no answer".to_owned(),
            Self::Tls(r) => r.to_string(),
            Self::Alert(alert) => format!(
                "the server refused this device's certificate with TLS's {alert} alert, before it \
                 proved to be your lmgw: Kai keeps the pairing and tries again"
            ),
            Self::Unproven => UNPROVEN.to_owned(),
            Self::KeyGone(why) => format!("this phone's key for lmgw is gone: {why}"),
            Self::KeyFailed(why) => format!("the Android Keystore could not sign: {why}"),
            Self::Other(why) => why.clone(),
        }
    }
}

/// `TlsRefusal::Unproven` at enrolment, in Kai's words: suggest, never state, a new link.
pub const UNPROVEN: &str = "the handshake failed: the server answered with TLS's \
     certificate_required alert before it proved to be the lmgw this link pairs with, so something \
     else may be answering at that address, or the pairing link may have ended (closed on lmgw's \
     Devices card, expired or used). Try again; if it keeps failing, make a new link on lmgw's \
     Devices card";

fn classify(e: &(dyn std::error::Error + 'static), enrolment: bool) -> Failure {
    if let Some(tls) = find_tls_error(e) {
        if let rustls::Error::General(msg) = tls {
            if let Some(why) = msg.strip_prefix(SIGN_GONE) {
                return Failure::KeyGone(why.to_owned());
            }
            if let Some(why) = msg.strip_prefix(SIGN_FAILED) {
                return Failure::KeyFailed(why.to_owned());
            }
        }
        let refusal = if enrolment {
            TlsRefusal::of_enrolment(tls)
        } else {
            TlsRefusal::of(tls)
        };
        match refusal {
            Some(TlsRefusal::Unproven) => return Failure::Unproven,
            Some(TlsRefusal::CertificateLapsed | TlsRefusal::CertificateRefused) => {
                if let rustls::Error::AlertReceived(d) = tls {
                    return Failure::Alert(alert_name(*d));
                }
            }
            Some(r) => return Failure::Tls(r),
            None => {}
        }
        if let rustls::Error::AlertReceived(d) = tls {
            return Failure::Other(format!(
                "the server ended the handshake with TLS's {} alert",
                alert_name(*d)
            ));
        }
        return Failure::Other(format!("TLS: {tls}"));
    }
    if let Some(io) = find_io(e) {
        use std::io::ErrorKind as K;
        match io.kind() {
            K::ConnectionRefused => return Failure::Refused,
            K::TimedOut | K::HostUnreachable | K::NetworkUnreachable => return Failure::NoAnswer,
            _ => {}
        }
    }
    if let Some(r) = find::<reqwest::Error>(e)
        && r.is_timeout()
    {
        return Failure::NoAnswer;
    }
    Failure::Other(describe(e))
}

/// The `io::Error` in `e`'s chain.
fn find_io<'a>(e: &'a (dyn std::error::Error + 'static)) -> Option<&'a std::io::Error> {
    find::<std::io::Error>(e)
}

fn find<'a, T: std::error::Error + 'static>(
    mut e: &'a (dyn std::error::Error + 'static),
) -> Option<&'a T> {
    loop {
        if let Some(t) = e.downcast_ref::<T>() {
            return Some(t);
        }
        e = e.source()?;
    }
}

/// TLS's name of an alert (RFC 8446 §6): rustls's variant in snake case (`BadCertificate` is
/// `bad_certificate`, `UnknownCA` `unknown_ca`).
pub fn alert_name(d: rustls::AlertDescription) -> String {
    let debug = format!("{d:?}");
    let mut out = String::with_capacity(debug.len() + 4);
    let mut prev_lower = false;
    for c in debug.chars() {
        if c.is_ascii_uppercase() {
            if prev_lower {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
            prev_lower = false;
        } else {
            out.push(c);
            prev_lower = c.is_ascii_lowercase() || c.is_ascii_digit();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alerts_are_named_as_tls_names_them() {
        assert_eq!(
            alert_name(rustls::AlertDescription::BadCertificate),
            "bad_certificate"
        );
        assert_eq!(
            alert_name(rustls::AlertDescription::UnknownCA),
            "unknown_ca"
        );
        assert_eq!(
            alert_name(rustls::AlertDescription::CertificateExpired),
            "certificate_expired"
        );
    }

    #[test]
    fn an_alert_about_the_certificate_is_said_never_a_refusal() {
        for (alert, name) in [
            (rustls::AlertDescription::BadCertificate, "bad_certificate"),
            (rustls::AlertDescription::UnknownCA, "unknown_ca"),
            (
                rustls::AlertDescription::CertificateExpired,
                "certificate_expired",
            ),
        ] {
            let e = rustls::Error::AlertReceived(alert);
            assert_eq!(Failure::of(&e), Failure::Alert(name.into()));
        }
        let e = rustls::Error::AlertReceived(rustls::AlertDescription::CertificateRequired);
        assert_eq!(Failure::of_enrolment(&e), Failure::Unproven);
        let e = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused");
        assert_eq!(Failure::of(&e), Failure::Refused);
        let e = rustls::Error::General(format!("{SIGN_GONE}no key"));
        assert_eq!(Failure::of(&e), Failure::KeyGone("no key".into()));
        let e = rustls::Error::General(format!("{SIGN_FAILED}busy"));
        assert_eq!(Failure::of(&e), Failure::KeyFailed("busy".into()));
    }
}
