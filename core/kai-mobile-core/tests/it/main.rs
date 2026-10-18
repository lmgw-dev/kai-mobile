//! The core's integration tests: one binary, one module per file. `pki` and `mock` are the
//! tests' device link and scripted lmgw, `rig` a core on the host.

mod conversation;
mod device_link;
mod feed;
mod held;
mod link;
mod mock;
mod pin;
mod pki;
mod rig;
mod settings;
mod voice;
