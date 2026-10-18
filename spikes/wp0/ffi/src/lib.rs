//! WP0's device spike, point (e) (Kai mobile design §17): what the FFI and the TLS path cost on
//! the phone. The spike app loads this library, which links kai-mobile-core whole, so the first
//! call into the real `Core` is measured with the real core's code; the measurements below are
//! spike-only exports and never reach the core's API.
//!
//! - [`sink_bench`]: an `EventSink` call (design §3.2) from the calling thread, from tokio's
//!   worker threads and from fresh native threads, so JNA's thread attach and detach shows as the
//!   difference;
//! - [`keystore_bench`]: a Keystore key generated and `SHA256withECDSA` signatures through the
//!   `Keystore` callback (§3.2, §4.3), the first one cold, each verified here with ring;
//! - [`tls_bench`]: full TLS 1.3 handshakes with client authentication against a loopback rustls
//!   server in this process, a test CA made at runtime, ring on both sides, the client configured
//!   by `lmgw_client::tls::client_config` with the Keystore key (§4.4).
//!
//! ```
//! let t = kai_wp0_spike::Timings::of(&[std::time::Duration::from_micros(3)]);
//! assert_eq!(t.count, 1);
//! assert_eq!(t.median_us, 3.0);
//! ```

mod keystore;
mod runtime;
mod sink;
mod timings;
mod tls;

pub use keystore::{Keystore, KeystoreError, KeystoreReport, keystore_bench};
pub use sink::{EventSink, SinkReport, sink_bench};
pub use timings::Timings;
pub use tls::{HandshakeTiming, TlsReport, tls_bench};

/// A spike measurement that could not run; the reason names the step.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum SpikeError {
    #[error("{reason}")]
    Failed { reason: String },
}

impl SpikeError {
    pub(crate) fn at(step: &str, e: impl std::fmt::Display) -> Self {
        Self::Failed {
            reason: format!("{step}: {e}"),
        }
    }
}

/// What the spike library is and how it was built (the core's own version comes from `Core`).
#[derive(Debug, Clone, uniffi::Record)]
pub struct SpikeInfo {
    pub spike_version: String,
    pub profile: String,
    pub target_arch: String,
    pub tokio_workers: u32,
}

/// The library's facts (also a cheap first call into the spike's own scaffolding).
#[uniffi::export]
pub fn spike_info() -> SpikeInfo {
    SpikeInfo {
        spike_version: env!("CARGO_PKG_VERSION").to_owned(),
        profile: if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
        .to_owned(),
        target_arch: std::env::consts::ARCH.to_owned(),
        tokio_workers: runtime::workers(),
    }
}

uniffi::setup_scaffolding!();

// The core's scaffolding lives in this library too (its Kotlin bindings load it): referenced here,
// or the linker leaves the core out, since the spike's own code calls none of it.
kai_mobile_core::uniffi_reexport_scaffolding!();
