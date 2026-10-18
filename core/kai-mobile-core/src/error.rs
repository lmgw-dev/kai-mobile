//! What Kotlin sees when a call fails (design §3.2): a kind, lmgw's code and lmgw's message
//! verbatim where lmgw refused, and the setting that bounded a wait where one did.

/// A failed call. Each message is for the owner's eyes and names the setting or file at fault.
#[derive(Debug, Clone, thiserror::Error, uniffi::Error)]
pub enum CoreError {
    // (Fields named `reason`, not `message`: UniFFI's Kotlin errors extend Throwable, whose
    // `message` a field of that name would redeclare.)
    /// A settings value outside what it can be; `key` is the setting's dotted name (§3.4).
    #[error("{key}: {reason}")]
    InvalidSetting { key: String, reason: String },
    /// The core's own files (settings, the pairing) could not be read or written.
    #[error("{what}: {reason}")]
    Storage { what: String, reason: String },
    /// The pairing link does not parse: the parser's words, which never echo a value from it.
    #[error("{reason}")]
    BadLink { reason: String },
    /// Pairing failed (design §4.2, §5.3): what failed, in the state's words.
    #[error("{reason}")]
    Pairing {
        kind: PairingFailure,
        reason: String,
    },
    /// The call needs a connection the phone does not have now: the connection state's words
    /// (design §5.3: answered at once, never a spinner).
    #[error("{reason}")]
    NotConnected { reason: String },
    /// The Kai conversation is not ready (design §6.1): the owner's steps.
    #[error("{reason}")]
    NotReady { reason: String },
    /// lmgw refused: its code and its message verbatim; `hint` adds what the phone knows (a
    /// call holds a concurrency slot, …).
    #[error("{reason} ({code})")]
    Refused {
        code: String,
        reason: String,
        hint: Option<String>,
    },
    /// lmgw did not answer in time, or the connection broke; `setting` is the bound's name.
    #[error("{reason}")]
    Unreachable {
        reason: String,
        setting: Option<String>,
    },
}

/// What failed at pairing (design §4.2, §5.3), each with its own words and action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum PairingFailure {
    /// 410 `pairing_token_used`: someone else used this code.
    CodeUsedElsewhere,
    /// 410 `pairing_token_expired`.
    LinkExpired,
    /// 401 `pairing_token_unknown`.
    LinkUnknown,
    /// No certificate lmgw sent hashes to `fp`, or the answer's CA is not the pinned one.
    NotThisLmgw,
    /// A `certificate_required` alert before lmgw proved itself (lmgw-client's
    /// `TlsRefusal::Unproven`): try again; a new link is suggested, never stated.
    Unproven,
    /// lmgw refused otherwise (`device_disabled`, `public_key_in_use`, `pairing_rate_limited`,
    /// `devices_ca_unavailable`, …): its code and message.
    Refused,
    /// No URL of the link answered.
    Unreachable,
    /// The Keystore could not make the device key.
    Keystore,
}
