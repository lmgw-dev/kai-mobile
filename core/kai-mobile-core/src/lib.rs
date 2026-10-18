//! Kai mobile's core (design §3): lmgw's protocol and its I/O behind a Kai-shaped API that Kotlin
//! sees through UniFFI. WP1 built the first `Core` (its version, the lmgw pin, the settings); WP2
//! adds the device link (pairing v2, enrolment with the Keystore key, the pinned client from one
//! constructor, the URL race), the held feed with its cursor and its notifications, and the Kai
//! conversation by text; WP3 voice over WebRTC (`voice`: the call bound by the Kai folder, its
//! events channel, the microphone's and Kai's track's decisions, the stop, the end's reason).
//!
//! `lib.rs` holds the exports and nothing else; each area is a module of its own (§3.1).
//!
//! ```
//! let pin = kai_mobile_core::LmgwPin::parse(
//!     "lmgw_commit 0123456789abcdef0123456789abcdef01234567\n\
//!      lmgw_tag unpublished\n\
//!      lmgw_tree 89abcdef0123456789abcdef0123456789abcdef\n",
//! )
//! .unwrap();
//! assert_eq!(pin.short_commit(), "0123456");
//! assert!(!pin.is_published());
//! ```

pub mod approvals;
pub mod clock;
pub mod conversation;
mod core;
mod error;
pub mod keystore;
pub mod link;
mod logging;
pub mod markdown;
pub mod notify;
pub mod pairing;
mod pin;
mod settings;
pub mod store;
pub mod views;
pub mod voice;

pub use core::{AppDirs, Core, CoreAbout, PlatformFacts};
pub use error::{CoreError, PairingFailure};
pub use keystore::{Keystore, KeystoreError};
pub use link::tls;
/// lmgw's sans-IO rules, which the core wraps (design M2).
pub use lmgw_client as lmgw;
pub use pin::LmgwPin;
pub use settings::{
    AppearanceSettings, Colours, ConversationSettings, EchoMode, FeedSettings, LmgwSettings,
    PairingSettings, Route, Settings, VoiceMode, VoiceSettings,
};
pub use voice::{
    AudioFocus, CallStart, EventSink, HeadsetPress, MicState, PlayoutStats, TransportState, Voice,
};

uniffi::setup_scaffolding!();
